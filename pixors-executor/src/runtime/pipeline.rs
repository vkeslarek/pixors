use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc::sync_channel, Arc};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;

use crate::data::Device;
use crate::error::Error;
use crate::gpu;
use crate::graph::graph::{ExecGraph, EdgePorts, StageId};
use crate::graph::item::Item;
use crate::operation::transfer::{Download, Upload};
use crate::operation::OperationNode;
use crate::stage::{Stage, StageNode};

use super::cpu::CpuChainRunner;
use super::event::PipelineEvent;
use super::gpu::GpuChainRunner;
use super::runner::{ItemReceiver, ItemSender, Runner, CHANNEL_BOUND};

type Graph = StableDiGraph<StageNode, EdgePorts>;

/// A compiled, runnable pipeline.
///
/// The execution graph is a **DAG** — stages may fan out to multiple
/// consumers (e.g. several sinks), or fan in from multiple producers.
///
/// The compiler fuses linear, same-device sequences into *chains*, each
/// running on its own thread. Inter-chain communication uses bounded
/// channels.
///
///  * **Fan-out**: the sending chain clones every emitted item to all
///    downstream channels.
///  * **Fan-in**: at run-time, multiple input channels are merged into a
///    single receiver (via lightweight forwarder threads) so runners only
///    ever see one input stream.
pub struct Pipeline {
    chains: Vec<(Box<dyn Runner>, Vec<ItemReceiver>, Vec<ItemSender>)>,
}

// ── Compile ─────────────────────────────────────────────────────────────────

impl Pipeline {
    pub fn compile(graph: &ExecGraph) -> Result<Self, Error> {
        let mut g: Graph = graph.graph.clone();
        let gpu_ok = gpu::gpu_available();

        // 1. Assign devices (Either stages promoted when adjacent to Gpu).
        let mut devs = assign_devices(&g, gpu_ok);

        // 2. Insert Upload/Download at device-crossing tile edges.
        insert_transfers(&mut g, &mut devs);

        // 3. Topological order.
        let order =
            toposort(&g, None).map_err(|_| Error::internal("pipeline graph has a cycle"))?;

        // 4. Fuse linear, same-device sequences into chains.
        let (chains, node_chain) = detect_chains(&g, &devs, &order);

        // 5. Create inter-chain channels.
        let (mut ch_in, mut ch_out) = build_channels(&g, &order, &node_chain, chains.len());

        // 6. Build a Runner per chain.
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

            let runner: Box<dyn Runner> = match dev {
                Device::Cpu | Device::Either => {
                    let kernels = nodes
                        .iter()
                        .map(|&id| {
                            g[id].cpu_kernel().ok_or_else(|| {
                                Error::internal(format!("'{}': no cpu_kernel", g[id].kind()))
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    Box::new(CpuChainRunner::new(kernels))
                }
                Device::Gpu => {
                    let steps = nodes
                        .iter()
                        .map(|&id| {
                            let s = &g[id];
                            if let Some(d) = s.gpu_kernel_descriptor() {
                                Ok(super::gpu::ChainStep::Gpu(d))
                            } else if let Some(k) = s.cpu_kernel() {
                                Ok(super::gpu::ChainStep::Cpu(k))
                            } else {
                                Err(Error::internal(format!("'{}': no kernel", s.kind())))
                            }
                        })
                        .collect::<Result<_, _>>()?;
                    Box::new(GpuChainRunner::new(steps))
                }
            };
            compiled.push((runner, inputs, outputs));
        }

        tracing::info!("[pixors] compile: {} chains built", compiled.len());
        Ok(Pipeline { chains: compiled })
    }
}

// ── Run ─────────────────────────────────────────────────────────────────────

impl Pipeline {
    /// Run the pipeline. Spawns one thread per chain; blocks until all finish.
    pub fn run(self, events: Option<std::sync::mpsc::Sender<PipelineEvent>>) -> Result<(), Error> {
        std::thread::scope(|s| {
            let mut handles = Vec::new();

            for (runner, inputs, outputs) in self.chains {
                // Fan-in: merge N input receivers into 1 so runners stay simple.
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

/// Merge multiple input receivers into a single `Vec` with 0 or 1 elements.
///
/// For 0 or 1 inputs, returns them unchanged (no extra threads).
/// For N > 1 inputs (fan-in), spawns N forwarder threads inside `scope`
/// that funnel all items into a shared channel.  EOS (`None`) is sent
/// only when the last forwarder finishes, so the runner sees a single
/// contiguous stream followed by one EOS.
fn merge_inputs<'scope, 'env: 'scope>(
    inputs: Vec<ItemReceiver>,
    scope: &'scope std::thread::Scope<'scope, 'env>,
) -> Vec<ItemReceiver> {
    if inputs.len() <= 1 {
        return inputs;
    }
    let n = inputs.len();
    let (tx, rx) = sync_channel::<Option<Item>>(CHANNEL_BOUND);
    let remaining = Arc::new(AtomicUsize::new(n));

    for recv in inputs {
        let tx = tx.clone();
        let remaining = remaining.clone();
        scope.spawn(move || {
            loop {
                match recv.recv() {
                    Ok(Some(item)) => {
                        if tx.send(Some(item)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        // This input exhausted. Send EOS only when we're the last.
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

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Assign each stage a device. `Either` stages adjacent to `Gpu` stages are
/// promoted to `Gpu` so they stay in the same chain and avoid transfers.
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

    // BFS: find connected components of Either nodes. If any member of a
    // component has a Gpu neighbour, promote the entire component.
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
        let touches_gpu = comp.iter().any(|&id| {
            neighbors(g, id).any(|n| devs[&n] == Device::Gpu)
        });
        if touches_gpu {
            for &id in comp {
                devs.insert(id, Device::Gpu);
            }
        }
    }
    devs
}

/// All direct neighbours (predecessors + successors) of a node.
fn neighbors(g: &Graph, id: StageId) -> impl Iterator<Item = StageId> + '_ {
    g.edges_directed(id, Direction::Outgoing)
        .map(|e| e.target())
        .chain(g.edges_directed(id, Direction::Incoming).map(|e| e.source()))
}

/// Insert Upload/Download stages at edges that cross a device boundary
/// and carry Tile data.
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
            .get(ports.from_port as usize)
            .is_some_and(|p| p.kind == crate::stage::DataKind::Tile);

        if sd != Device::Gpu && dd == Device::Gpu && is_tile {
            let mid = g.add_node(StageNode::Operation(OperationNode::Upload(Upload)));
            if let Some(e) = g.find_edge(src, dst) { g.remove_edge(e); }
            g.add_edge(src, mid, ports);
            g.add_edge(mid, dst, ports);
            devs.insert(mid, Device::Cpu);
        } else if sd == Device::Gpu && dd != Device::Gpu && is_tile {
            let mid = g.add_node(StageNode::Operation(OperationNode::Download(Download)));
            if let Some(e) = g.find_edge(src, dst) { g.remove_edge(e); }
            g.add_edge(src, mid, ports);
            g.add_edge(mid, dst, ports);
            devs.insert(mid, Device::Cpu);
        }
    }
}

/// Fuse consecutive, same-device, single-edge sequences into chains.
/// Returns `(chains, node→chain_index)`.
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
            let pred = g.edges_directed(id, Direction::Incoming).next().unwrap().source();
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

/// Create bounded channels between chains. Each unique (src_chain, dst_chain)
/// pair gets one channel. Fan-out is handled by runners cloning to all
/// output senders; fan-in is handled by `merge_inputs` at run-time.
fn build_channels(
    g: &Graph,
    order: &[StageId],
    node_chain: &HashMap<StageId, usize>,
    num_chains: usize,
) -> (Vec<Vec<ItemReceiver>>, Vec<Vec<ItemSender>>) {
    let mut ins: Vec<Vec<ItemReceiver>> = (0..num_chains).map(|_| vec![]).collect();
    let mut outs: Vec<Vec<ItemSender>> = (0..num_chains).map(|_| vec![]).collect();
    let mut seen = HashMap::new();

    for &src_node in order {
        let sc = node_chain[&src_node];
        for edge in g.edges_directed(src_node, Direction::Outgoing) {
            let dc = node_chain[&edge.target()];
            if sc == dc || seen.contains_key(&(sc, dc)) {
                continue;
            }
            seen.insert((sc, dc), ());
            let (tx, rx) = sync_channel::<Option<Item>>(CHANNEL_BOUND);
            outs[sc].push(tx);
            ins[dc].push(rx);
        }
    }
    (ins, outs)
}
