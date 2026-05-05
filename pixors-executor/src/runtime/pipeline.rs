use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc::sync_channel};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;

use crate::data::device::Device;
use crate::error::Error;
use crate::gpu;
use crate::graph::graph::{EdgePorts, ExecGraph, StageId};
use crate::operation::OperationNode;
use crate::operation::transfer::download::Download;
use crate::operation::transfer::upload::Upload;
use crate::stage::{PortGroup, Stage, StageNode};

use super::chain::ChainRunner;
use super::event::PipelineEvent;
use super::runner::{CHANNEL_BOUND, ItemReceiver, ItemSender, RoutedItem, Runner};

type Graph = StableDiGraph<StageNode, EdgePorts>;

pub struct Pipeline {
    chains: Vec<(
        Box<dyn Runner>,
        Vec<ItemReceiver>,
        Vec<(ItemSender, u16, u16)>,
    )>,
}

// ── Compile ─────────────────────────────────────────────────────────────────

impl Pipeline {
    pub fn compile(graph: &ExecGraph) -> Result<Self, Error> {
        let mut g: Graph = graph.graph.clone();
        let gpu_ok = gpu::context::gpu_available();

        validate_ports(&g)?;

        let mut devs = assign_devices(&g, gpu_ok);

        insert_transfers(&mut g, &mut devs);

        let order =
            toposort(&g, None).map_err(|_| Error::internal("pipeline graph has a cycle"))?;

        let (chains, node_chain) = detect_chains(&g, &devs, &order);

        let (mut ch_in, mut ch_out) = build_channels(&g, &order, &node_chain, chains.len());

        let mut compiled = Vec::new();
        for (idx, nodes) in chains.iter().enumerate() {
            let dev = devs[&nodes[0]];
            let inputs = std::mem::take(&mut ch_in[idx]);
            let outputs = std::mem::take(&mut ch_out[idx]);

            tracing::info!(
                "[pixors] compile: chain[{idx}] device={dev:?} {} stage(s): {:?}",
                nodes.len(),
                nodes.iter().map(|&id| g[id].kind()).collect::<Vec<_>>(),
            );

            let kernels = nodes
                .iter()
                .map(|&id| {
                    g[id]
                        .processor()
                        .ok_or_else(|| Error::internal(format!("'{}': no kernel", g[id].kind())))
                })
                .collect::<Result<_, _>>()?;
            compiled.push((
                Box::new(ChainRunner::new(kernels, dev)) as Box<dyn Runner>,
                inputs,
                outputs,
            ));
        }

        tracing::info!("[pixors] compile: {} chains built", compiled.len());
        Ok(Pipeline { chains: compiled })
    }
}

// ── Run ─────────────────────────────────────────────────────────────────────

impl Pipeline {
    pub fn run(self, events: Option<std::sync::mpsc::Sender<PipelineEvent>>) -> Result<(), Error> {
        std::thread::scope(|s| {
            let mut handles = Vec::new();

            for (runner, inputs, outputs) in self.chains {
                let merged = merge_inputs(inputs, s);

                let h = s.spawn(move || runner.run(merged, outputs));
                handles.push(h);
            }

            let mut first_err: Option<Error> = None;
            for h in handles {
                match h.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        if let Some(ref tx) = events {
                            let _ = tx.send(PipelineEvent::Error(e.to_string()));
                        }
                        first_err.get_or_insert(e);
                    }
                    Err(_) => {
                        first_err.get_or_insert(Error::internal("pipeline thread panicked"));
                    }
                }
            }

            if let Some(ref tx) = events {
                let _ = tx.send(PipelineEvent::Done);
            }
            first_err.map(Err).unwrap_or(Ok(()))
        })
    }
}

fn merge_inputs<'scope, 'env: 'scope>(
    inputs: Vec<ItemReceiver>,
    scope: &'scope std::thread::Scope<'scope, 'env>,
) -> Vec<ItemReceiver> {
    if inputs.len() <= 1 {
        return inputs;
    }
    let n = inputs.len();
    let (tx, rx) = sync_channel::<Option<RoutedItem>>(CHANNEL_BOUND);
    let remaining = Arc::new(AtomicUsize::new(n));

    for recv in inputs {
        let tx = tx.clone();
        let remaining = remaining.clone();
        scope.spawn(move || {
            loop {
                match recv.recv() {
                    Ok(Some(routed)) => {
                        if tx.send(Some(routed)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
                            let _ = tx.send(None);
                        }
                        break;
                    }
                }
            }
        });
    }
    vec![rx]
}

// ── Port validation ─────────────────────────────────────────────────────────

fn validate_ports(g: &Graph) -> Result<(), Error> {
    for edge in g.edge_indices() {
        let (src, dst) = g.edge_endpoints(edge).unwrap();
        let ports = *g.edge_weight(edge).unwrap();
        let src_spec = g[src].ports();
        let dst_spec = g[dst].ports();

        let src_output_count = outport_count(g, src, &src_spec.outputs);
        if ports.from_port as usize >= src_output_count {
            return Err(Error::internal(format!(
                "edge {} -> {}: from_port {} out of bounds (outputs have {} ports)",
                g[src].kind(),
                g[dst].kind(),
                ports.from_port,
                src_output_count,
            )));
        }

        let dst_input_count = inport_count(g, dst, &dst_spec.inputs);
        if ports.to_port as usize >= dst_input_count {
            return Err(Error::internal(format!(
                "edge {} -> {}: to_port {} out of bounds (inputs have {} ports)",
                g[src].kind(),
                g[dst].kind(),
                ports.to_port,
                dst_input_count,
            )));
        }

        let src_kind = src_spec.outputs.kind_at(ports.from_port as usize);
        let dst_kind = dst_spec.inputs.kind_at(ports.to_port as usize);
        if let (Some(sk), Some(dk)) = (src_kind, dst_kind)
            && sk != dk
        {
            return Err(Error::internal(format!(
                "edge {} -> {}: DataKind mismatch (output {:?}, input {:?})",
                g[src].kind(),
                g[dst].kind(),
                sk,
                dk,
            )));
        }
    }
    Ok(())
}

fn outport_count(g: &Graph, id: StageId, group: &PortGroup) -> usize {
    match group {
        PortGroup::Fixed(ports) => ports.len(),
        PortGroup::Variable(_) => g
            .edges_directed(id, Direction::Outgoing)
            .map(|e| e.weight().from_port as usize + 1)
            .max()
            .unwrap_or(0),
    }
}

fn inport_count(g: &Graph, id: StageId, group: &PortGroup) -> usize {
    match group {
        PortGroup::Fixed(ports) => ports.len(),
        PortGroup::Variable(_) => g
            .edges_directed(id, Direction::Incoming)
            .map(|e| e.weight().to_port as usize + 1)
            .max()
            .unwrap_or(0),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn assign_devices(g: &Graph, gpu_ok: bool) -> HashMap<StageId, Device> {
    let mut devs: HashMap<StageId, Device> = g
        .node_indices()
        .map(|id| {
            let d = match g[id].device() {
                Device::Gpu if !gpu_ok => Device::Cpu,
                other => other,
            };
            (id, d)
        })
        .collect();

    if !gpu_ok {
        return devs;
    }

    let mut visited = HashMap::<StageId, bool>::new();
    let mut components: Vec<Vec<StageId>> = Vec::new();

    for id in g.node_indices() {
        if devs[&id] != Device::Either || visited.contains_key(&id) {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![id];
        visited.insert(id, true);
        while let Some(cur) = stack.pop() {
            comp.push(cur);
            for n in neighbors(g, cur) {
                if devs[&n] == Device::Either && !visited.contains_key(&n) {
                    visited.insert(n, true);
                    stack.push(n);
                }
            }
        }
        components.push(comp);
    }

    for comp in &components {
        let touches_gpu = comp
            .iter()
            .any(|&id| neighbors(g, id).any(|n| devs[&n] == Device::Gpu));
        if touches_gpu {
            for &id in comp {
                devs.insert(id, Device::Gpu);
            }
        }
    }
    devs
}

fn neighbors(g: &Graph, id: StageId) -> impl Iterator<Item = StageId> + '_ {
    g.edges_directed(id, Direction::Outgoing)
        .map(|e| e.target())
        .chain(
            g.edges_directed(id, Direction::Incoming)
                .map(|e| e.source()),
        )
}

fn insert_transfers(g: &mut Graph, devs: &mut HashMap<StageId, Device>) {
    let edges: Vec<_> = g
        .edge_indices()
        .map(|e| {
            let (s, d) = g.edge_endpoints(e).unwrap();
            (s, d, *g.edge_weight(e).unwrap())
        })
        .collect();

    for (src, dst, ports) in edges {
        let sd = devs[&src];
        let dd = devs[&dst];
        let is_tile = g[src]
            .ports()
            .outputs
            .kind_at(ports.from_port as usize)
            .is_some_and(|k| k == crate::stage::DataKind::Tile);

        if sd != Device::Gpu && dd == Device::Gpu && is_tile {
            let mid = g.add_node(StageNode::Operation(OperationNode::Upload(Upload)));
            if let Some(e) = g.find_edge(src, dst) {
                g.remove_edge(e);
            }
            g.add_edge(
                src,
                mid,
                EdgePorts {
                    from_port: ports.from_port,
                    to_port: 0,
                },
            );
            g.add_edge(
                mid,
                dst,
                EdgePorts {
                    from_port: 0,
                    to_port: ports.to_port,
                },
            );
            devs.insert(mid, Device::Cpu);
        } else if sd == Device::Gpu && dd != Device::Gpu && is_tile {
            let mid = g.add_node(StageNode::Operation(OperationNode::Download(Download)));
            if let Some(e) = g.find_edge(src, dst) {
                g.remove_edge(e);
            }
            g.add_edge(
                src,
                mid,
                EdgePorts {
                    from_port: ports.from_port,
                    to_port: 0,
                },
            );
            g.add_edge(
                mid,
                dst,
                EdgePorts {
                    from_port: 0,
                    to_port: ports.to_port,
                },
            );
            devs.insert(mid, Device::Cpu);
        }
    }
}

fn detect_chains(
    g: &Graph,
    devs: &HashMap<StageId, Device>,
    order: &[StageId],
) -> (Vec<Vec<StageId>>, HashMap<StageId, usize>) {
    let mut node_chain: HashMap<StageId, usize> = HashMap::new();
    let mut chains: Vec<Vec<StageId>> = Vec::new();

    for &id in order {
        let dev = devs[&id];
        let merged_into = if g.edges_directed(id, Direction::Incoming).count() == 1 {
            let pred = g
                .edges_directed(id, Direction::Incoming)
                .next()
                .unwrap()
                .source();
            let pred_out = g.edges_directed(pred, Direction::Outgoing).count();
            let same_device = (devs[&pred] != Device::Gpu) == (dev != Device::Gpu);
            if pred_out == 1 && same_device {
                Some(node_chain[&pred])
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ci) = merged_into {
            chains[ci].push(id);
            node_chain.insert(id, ci);
        } else {
            let ci = chains.len();
            chains.push(vec![id]);
            node_chain.insert(id, ci);
        }
    }
    (chains, node_chain)
}

fn build_channels(
    g: &Graph,
    order: &[StageId],
    node_chain: &HashMap<StageId, usize>,
    num_chains: usize,
) -> (Vec<Vec<ItemReceiver>>, Vec<Vec<(ItemSender, u16, u16)>>) {
    let mut ins: Vec<Vec<ItemReceiver>> = (0..num_chains).map(|_| vec![]).collect();
    let mut outs: Vec<Vec<(ItemSender, u16, u16)>> = (0..num_chains).map(|_| vec![]).collect();
    let mut seen = HashMap::new();

    for &src_node in order {
        let sc = node_chain[&src_node];
        for edge in g.edges_directed(src_node, Direction::Outgoing) {
            let dc = node_chain[&edge.target()];
            let ports = *edge.weight();
            if sc == dc || seen.contains_key(&(sc, dc, ports.from_port, ports.to_port)) {
                continue;
            }
            seen.insert((sc, dc, ports.from_port, ports.to_port), ());
            let (tx, rx) = sync_channel::<Option<RoutedItem>>(CHANNEL_BOUND);
            outs[sc].push((tx, ports.from_port, ports.to_port));
            ins[dc].push(rx);
        }
    }
    (ins, outs)
}
