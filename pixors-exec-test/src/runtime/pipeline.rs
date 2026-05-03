use std::collections::HashMap;
use std::sync::mpsc::sync_channel;

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::visit::EdgeRef;

use crate::error::Error;
use crate::gpu;
use crate::graph::graph::{ExecGraph, StageId};
use crate::graph::item::Item;
use crate::operation::transfer::{Download, Upload};
use crate::operation::OperationNode;
use crate::stage::{Stage, StageNode};

use super::cpu::CpuChainRunner;
use super::gpu::GpuChainRunner;
use super::runner::{ItemReceiver, ItemSender, Runner, CHANNEL_BOUND};

/// A compiled, runnable pipeline.
pub struct Pipeline {
    /// (runner, input_receivers, output_senders) — in topological order.
    chains: Vec<(Box<dyn Runner>, Vec<ItemReceiver>, Vec<ItemSender>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Device {
    Cpu,
    Gpu,
}

impl Pipeline {
    /// Compile an `ExecGraph` into a `Pipeline`.
    pub fn compile(graph: &ExecGraph) -> Result<Self, Error> {
        use petgraph::stable_graph::StableDiGraph;

        let mut g: StableDiGraph<StageNode, crate::graph::graph::EdgePorts> =
            graph.graph.clone();

        let gpu_available = gpu::gpu_available();

        // --- Assign devices ---
        let device_map: HashMap<StageId, Device> = g
            .node_indices()
            .map(|id| {
                let stage = &g[id];
                let dev = if gpu_available
                    && stage.gpu_kernel_descriptor().is_some()
                    && stage.hints().prefers_gpu
                {
                    Device::Gpu
                } else {
                    Device::Cpu
                };
                (id, dev)
            })
            .collect();

        // --- Insert Upload/Download on device-crossing edges ---
        let edges: Vec<(StageId, StageId, crate::graph::graph::EdgePorts)> = g
            .edge_indices()
            .map(|e| {
                let (src, dst) = g.edge_endpoints(e).unwrap();
                (src, dst, *g.edge_weight(e).unwrap())
            })
            .collect();

        let mut updated_device_map = device_map.clone();

        for (src, dst, ports) in edges {
            let src_dev = device_map[&src];
            let dst_dev = device_map[&dst];
            if src_dev == Device::Cpu && dst_dev == Device::Gpu {
                let upload_id = g.add_node(StageNode::Operation(OperationNode::Upload(Upload)));
                if let Some(e) = g.find_edge(src, dst) {
                    g.remove_edge(e);
                }
                g.add_edge(src, upload_id, ports);
                g.add_edge(upload_id, dst, ports);
                updated_device_map.insert(upload_id, Device::Cpu);
            } else if src_dev == Device::Gpu && dst_dev == Device::Cpu {
                let download_id =
                    g.add_node(StageNode::Operation(OperationNode::Download(Download)));
                if let Some(e) = g.find_edge(src, dst) {
                    g.remove_edge(e);
                }
                g.add_edge(src, download_id, ports);
                g.add_edge(download_id, dst, ports);
                updated_device_map.insert(download_id, Device::Cpu);
            }
        }

        let device_map = updated_device_map;

        // --- Topological order ---
        let order =
            toposort(&g, None).map_err(|_| Error::internal("pipeline graph has a cycle"))?;

        // --- Chain detection: merge consecutive same-device nodes with no branching ---
        let mut node_to_chain: HashMap<StageId, usize> = HashMap::new();
        let mut chain_order: Vec<Vec<StageId>> = Vec::new();

        for &id in &order {
            let dev = device_map[&id];
            let in_degree = g.edges_directed(id, Direction::Incoming).count();

            let merged = if in_degree == 1 {
                let pred = g
                    .edges_directed(id, Direction::Incoming)
                    .next()
                    .map(|e| e.source())
                    .unwrap();
                let pred_out = g.edges_directed(pred, Direction::Outgoing).count();
                let pred_dev = device_map[&pred];
                if pred_out == 1 && pred_dev == dev {
                    Some(node_to_chain[&pred])
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(chain_idx) = merged {
                chain_order[chain_idx].push(id);
                node_to_chain.insert(id, chain_idx);
            } else {
                let chain_idx = chain_order.len();
                chain_order.push(vec![id]);
                node_to_chain.insert(id, chain_idx);
            }
        }

        // --- Build channels between chains ---
        let num_chains = chain_order.len();
        let mut chain_outputs: Vec<Vec<ItemSender>> = (0..num_chains).map(|_| vec![]).collect();
        let mut chain_inputs: Vec<Vec<ItemReceiver>> = (0..num_chains).map(|_| vec![]).collect();

        let mut channel_pairs: HashMap<(usize, usize), ()> = HashMap::new();

        for &src_node in &order {
            let src_chain = node_to_chain[&src_node];
            for edge in g.edges_directed(src_node, Direction::Outgoing) {
                let dst_node = edge.target();
                let dst_chain = node_to_chain[&dst_node];
                if src_chain == dst_chain {
                    continue;
                }
                let pair = (src_chain, dst_chain);
                if channel_pairs.contains_key(&pair) {
                    continue;
                }
                channel_pairs.insert(pair, ());
                let (tx, rx) = sync_channel::<Option<Item>>(CHANNEL_BOUND);
                chain_outputs[src_chain].push(tx);
                chain_inputs[dst_chain].push(rx);
            }
        }

        // --- Build runners ---
        let mut compiled_chains: Vec<(Box<dyn Runner>, Vec<ItemReceiver>, Vec<ItemSender>)> =
            Vec::new();

        for (chain_idx, nodes) in chain_order.iter().enumerate() {
            let dev = device_map[&nodes[0]];
            let inputs = std::mem::take(&mut chain_inputs[chain_idx]);
            let outputs = std::mem::take(&mut chain_outputs[chain_idx]);

            let runner: Box<dyn Runner> = match dev {
                Device::Cpu => {
                    let mut kernels = Vec::new();
                    for &node_id in nodes {
                        let stage = &g[node_id];
                        let kernel = stage
                            .cpu_kernel()
                            .ok_or_else(|| {
                                Error::internal(format!(
                                    "stage '{}' assigned to CPU but has no cpu_kernel()",
                                    stage.kind()
                                ))
                            })?;
                        kernels.push(kernel);
                    }
                    Box::new(CpuChainRunner::new(kernels))
                }
                Device::Gpu => {
                    let mut descs = Vec::new();
                    for &node_id in nodes {
                        let stage = &g[node_id];
                        let desc = stage.gpu_kernel_descriptor().ok_or_else(|| {
                            Error::internal(format!(
                                "stage '{}' assigned to GPU but has no gpu_kernel_descriptor()",
                                stage.kind()
                            ))
                        })?;
                        descs.push(desc);
                    }
                    Box::new(GpuChainRunner::new(descs))
                }
            };

            compiled_chains.push((runner, inputs, outputs));
        }

        Ok(Pipeline { chains: compiled_chains })
    }

    /// Run the pipeline. Spawns one thread per chain; blocks until all finish.
    pub fn run(self) -> Result<(), Error> {
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for (runner, inputs, outputs) in self.chains {
                let h = s.spawn(move || runner.run(inputs, outputs));
                handles.push(h);
            }
            let mut first_error: Option<Error> = None;
            for h in handles {
                match h.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        if first_error.is_none() {
                            first_error = Some(e);
                        }
                    }
                    Err(_) => {
                        if first_error.is_none() {
                            first_error = Some(Error::internal("pipeline thread panicked"));
                        }
                    }
                }
            }
            first_error.map(Err).unwrap_or(Ok(()))
        })
    }
}
