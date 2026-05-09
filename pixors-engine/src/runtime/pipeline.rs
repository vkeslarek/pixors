use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, mpsc::sync_channel};
use std::thread::{self, JoinHandle};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;

use crate::data::device::Device;
use crate::error::Error;
use crate::gpu;
use crate::graph::graph::{EdgePorts, ExecGraph, StageArc, StageId};
use crate::operation::transfer::download::Download;
use crate::operation::transfer::upload::Upload;
use crate::stage::PortGroup;

use super::chain::{ChainRunner, ProgressState};
use super::event::PipelineEvent;
use super::runner::{CHANNEL_BOUND, ItemReceiver, ItemSender, RoutedItem, Runner};

type Graph = StableDiGraph<StageArc, EdgePorts>;

pub struct Pipeline {
    chains: Vec<(
        Box<dyn Runner>,
        Vec<ItemReceiver>,
        Vec<(ItemSender, u16, u16)>,
    )>,
    cancelled: Arc<AtomicBool>,
}

// ── Compile ─────────────────────────────────────────────────────────────────

impl Pipeline {
    pub fn compile(
        graph: &ExecGraph,
        progress_tx: Option<SyncSender<PipelineEvent>>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, Error> {
        let mut g: Graph = graph.graph.clone();
        let gpu_ok = gpu::context::gpu_available();
        let gpu_ctx = if gpu_ok {
            gpu::context::try_init()
        } else {
            None
        };

        validate_ports(&g)?;

        let mut devs = assign_devices(&g, gpu_ok);

        insert_transfers(&mut g, &mut devs);

        let order =
            toposort(&g, None).map_err(|_| Error::internal("pipeline graph has a cycle"))?;

        let (chains, node_chain) = detect_chains(&g, &devs, &order);

        let (mut ch_in, mut ch_out) = build_channels(&g, &order, &node_chain, chains.len());

        let total_work = compute_work_total(&g, &order);

        let progress = match progress_tx {
            Some(ref tx) if total_work > 0 => Some(Arc::new(ProgressState {
                done: AtomicUsize::new(0),
                total: total_work,
                tx: tx.clone(),
            })),
            _ => None,
        };

        let mut compiled = Vec::new();
        for (idx, nodes) in chains.iter().enumerate() {
            let dev = devs[&nodes[0]];
            let inputs = std::mem::take(&mut ch_in[idx]);
            let outputs = std::mem::take(&mut ch_out[idx]);
            let num_stages = nodes.len();
            let kinds: Vec<_> = nodes.iter().map(|&id| g[id].kind()).collect();

            let chain_name = format!("#{idx} [{dev:?}]  {}", kinds.join(" → "));
            tracing::info!(
                "[pixors] compile: {chain_name}  ({} stage{})",
                num_stages,
                if num_stages == 1 { "" } else { "s" },
            );

            let producer = nodes.first().and_then(|&id| g[id].producer());
            let consumer = nodes.last().and_then(|&id| g[id].consumer());
            let mid_start = if producer.is_some() { 1 } else { 0 };
            let mid_end = nodes.len() - if consumer.is_some() { 1 } else { 0 };
            let kernels: Vec<Box<dyn crate::stage::Processor>> = nodes[mid_start..mid_end]
                .iter()
                .map(|&id| {
                    g[id]
                        .processor()
                        .ok_or_else(|| Error::internal(format!("'{}': no kernel", g[id].kind())))
                })
                .collect::<Result<_, _>>()?;
            compiled.push((
                Box::new(ChainRunner::new(
                    producer,
                    kernels,
                    consumer,
                    dev,
                    gpu_ctx.clone(),
                    progress.clone(),
                    chain_name,
                    cancelled.clone(),
                )) as Box<dyn Runner>,
                inputs,
                outputs,
            ));
        }

        tracing::info!(
            "[pixors] compile: {} chains built, total_work={total_work}",
            compiled.len()
        );
        Ok(Pipeline {
            chains: compiled,
            cancelled,
        })
    }
}

// ── Run ─────────────────────────────────────────────────────────────────────

pub struct PipelineHandle {
    cancelled: Arc<AtomicBool>,
    handles: Vec<JoinHandle<Result<(), Error>>>,
}

impl PipelineHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn join(self) -> Result<(), Error> {
        let mut first_err: Option<Error> = None;
        for (i, h) in self.handles.into_iter().enumerate() {
            match h.join() {
                Ok(Ok(())) => {
                    tracing::info!("[pixors] pipeline: thread {i} joined OK");
                }
                Ok(Err(e)) => {
                    tracing::error!("[pixors] pipeline: thread {i} returned error: {e}");
                    first_err.get_or_insert(e);
                }
                Err(_) => {
                    tracing::error!("[pixors] pipeline: thread {i} PANICKED");
                    first_err.get_or_insert(Error::internal("pipeline thread panicked"));
                }
            }
        }
        first_err.map(Err).unwrap_or(Ok(()))
    }
}

impl Pipeline {
    pub fn run(self, events: Option<SyncSender<PipelineEvent>>) -> PipelineHandle {
        let cancelled = self.cancelled;
        let mut handles = Vec::new();

        for (runner, inputs, outputs) in self.chains {
            let cancelled_clone = cancelled.clone();
            let merged = merge_inputs(inputs);
            let events_clone = events.clone();

            let h = thread::spawn(move || {
                let result = runner.run(merged, outputs);
                if let Err(ref e) = result
                    && let Some(ref tx) = events_clone
                {
                    let _ = tx.send(PipelineEvent::Error {
                        tag: 0,
                        message: e.to_string(),
                    });
                }
                if cancelled_clone.load(Ordering::Relaxed)
                    && let Some(ref tx) = events_clone
                {
                    let _ = tx.send(PipelineEvent::Cancelled { tag: 0 });
                }
                result
            });
            handles.push(h);
        }

        PipelineHandle { cancelled, handles }
    }
}

fn merge_inputs(inputs: Vec<ItemReceiver>) -> Vec<ItemReceiver> {
    if inputs.len() <= 1 {
        return inputs;
    }
    let n = inputs.len();
    let (tx, rx) = sync_channel::<Option<RoutedItem>>(CHANNEL_BOUND);
    let remaining = Arc::new(AtomicUsize::new(n));

    for recv in inputs {
        let tx = tx.clone();
        let remaining = remaining.clone();
        thread::spawn(move || {
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
    use petgraph::visit::EdgeRef;
    let mut devs: HashMap<StageId, Device> = HashMap::new();

    // Pass 1: fixed devices (Cpu, Gpu). Downgrade Gpu→Cpu if unavailable.
    for id in g.node_indices() {
        let h = g[id].hints();
        match h.device {
            Device::Cpu => {
                devs.insert(id, Device::Cpu);
            }
            Device::Gpu => {
                devs.insert(id, if gpu_ok { Device::Gpu } else { Device::Cpu });
            }
            Device::Either => {
                // Left unassigned — resolved in pass 2
            }
        }
    }

    // Pass 2: iterative assignment for Either stages. Minimize CPU↔GPU transfers.
    loop {
        let mut assigned_any = false;

        for id in g.node_indices() {
            if devs.contains_key(&id) {
                continue;
            }
            let hints = g[id].hints();

            // Collect assigned neighbours
            let mut adj_devs: Vec<Device> = Vec::new();
            for edge in g.edges(id) {
                let other = if edge.source() == id {
                    edge.target()
                } else {
                    edge.source()
                };
                if let Some(&d) = devs.get(&other) {
                    adj_devs.push(d);
                }
            }

            if adj_devs.is_empty() {
                // No neighbours assigned yet — defer to next iteration
                continue;
            }

            let first = adj_devs[0];
            let all_same = adj_devs.iter().all(|&d| d == first);

            // Rule 2a/2b: honour stage preference, downgrade GPU→CPU if unavailable
            if let Some(pref) = hints.preference {
                let effective = if pref == Device::Gpu && !gpu_ok {
                    Device::Cpu
                } else {
                    pref
                };
                devs.insert(id, effective);
                assigned_any = true;
                continue;
            }

            // Rule 2c: all adjacents on same device → assign to that device
            if all_same {
                devs.insert(id, first);
                assigned_any = true;
                continue;
            }

            // Rule 2d: conflicting adjacents → default to GPU (or CPU if unavailable)
            devs.insert(id, if gpu_ok { Device::Gpu } else { Device::Cpu });
            assigned_any = true;
        }

        if !assigned_any {
            break;
        }
    }

    // Pass 3: any remaining unassigned → GPU if available
    for id in g.node_indices() {
        devs.entry(id)
            .or_insert_with(|| if gpu_ok { Device::Gpu } else { Device::Cpu });
    }

    devs
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
        let data_kind = g[src].ports().outputs.kind_at(ports.from_port as usize);

        let is_transferrable = data_kind.is_some_and(|k| k == crate::stage::DataKind::Tile)
            || data_kind.is_some_and(|k| k == crate::stage::DataKind::Neighborhood);

        if sd != Device::Gpu && dd == Device::Gpu && is_transferrable {
            let mid = g.add_node(Arc::new(Upload));
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
        } else if sd == Device::Gpu && dd != Device::Gpu && is_transferrable {
            let mid = g.add_node(Arc::new(Download));
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

fn compute_work_total(g: &Graph, order: &[StageId]) -> usize {
    let mut output_work: HashMap<StageId, usize> = HashMap::new();
    let mut total_received = 0usize;

    for &id in order {
        let stage = &g[id];
        let input_work = g
            .edges_directed(id, Direction::Incoming)
            .map(|e| output_work.get(&e.source()).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);

        let received = if g.edges_directed(id, Direction::Incoming).count() == 0 {
            1
        } else {
            input_work
        };
        total_received += received;

        let ow = if g.edges_directed(id, Direction::Incoming).count() == 0 {
            stage.source_items()
        } else {
            (input_work as f64 * stage.work_multiplier()).ceil() as usize
        };
        output_work.insert(id, ow);
    }

    tracing::info!(
        "[pixors] compute_work_total: total_received={}",
        total_received
    );
    total_received
}
