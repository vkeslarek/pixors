use std::collections::HashMap;

use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use petgraph::Direction;

use crate::pipeline::state_graph::cache::CacheIndex;
use crate::pipeline::state_graph::graph::{NodeId, StateGraph};
use crate::pipeline::state::{ExpandCtx, ExpansionOption, StateNodeTrait};

use crate::debug_stopwatch;
use crate::pipeline::exec_graph::graph::{ExecEdgePorts, ExecGraph, StageId};
use crate::pipeline::exec::{Device, ExecNode, Stage};
use crate::pipeline::exec;

#[derive(Debug, Clone)]
pub enum CompileError {
    ValidationFailed(String),
    CycleDetected,
}

/// Run-mode the compiler targets. `Preview` may eventually skip stages or
/// downsample to the requested mip level; today both modes produce the same
/// stage list.
#[derive(Debug, Clone, Copy)]
pub enum ExecutionMode {
    Apply { force_cpu: bool },
    Preview { force_cpu: bool, viewport_mip: u32 },
}

impl ExecutionMode {
    pub fn force_cpu(&self) -> bool {
        match self {
            ExecutionMode::Apply { force_cpu } => *force_cpu,
            ExecutionMode::Preview { force_cpu, .. } => *force_cpu,
        }
    }
}

/// Lower a `StateGraph` into a flat, executable `ExecGraph`.
///
/// For each `StateNode`, asks `expand` for one or more `ExpansionOption`s
/// (each tied to a single device). A greedy walk in topological order picks
/// the option that minimizes the number of CPU↔GPU transitions across
/// inter-node edges, breaking ties by `prefer` (higher wins). At inter-node
/// edges where adjacent options sit on different devices, the compiler
/// inserts a `Upload` (Cpu→Gpu) or `Download` (Gpu→Cpu) stage.
pub fn compile(
    spec: &StateGraph,
    _mode: ExecutionMode,
    _cache_index: &CacheIndex,
    gpu_available: bool,
) -> Result<ExecGraph, CompileError> {
    spec.validate()
        .map_err(|e| CompileError::ValidationFailed(format!("{:?}", e)))?;
    let _sw = debug_stopwatch!("compile");
    let topo = spec
        .topological_order()
        .map_err(|_| CompileError::CycleDetected)?;

    tracing::info!("[pixors] compile: gpu_available={}", gpu_available);
    let ctx = ExpandCtx { gpu_available };

    // Per-node: chosen option's device.
    let mut chosen_device: HashMap<NodeId, Device> = HashMap::with_capacity(topo.len());
    // Per-node: chosen option's stages (taken out of the option vec).
    let mut chosen_stages: HashMap<NodeId, Vec<ExecNode>> = HashMap::with_capacity(topo.len());

    for &node_id in &topo {
        let options = spec.graph[node_id].expand(&ctx);
        if options.is_empty() {
            continue;
        }
        // Score each option by transition cost vs already-chosen predecessors.
        let pred_devices: Vec<Device> = spec
            .graph
            .edges_directed(node_id, Direction::Incoming)
            .filter_map(|er| chosen_device.get(&er.source()).copied())
            .collect();
        let chosen = pick_option(options, &pred_devices);
        tracing::info!(
            "[pixors] compile: node {} -> device {:?} (stages: {:?})",
            spec.graph[node_id].kind(),
            chosen.device,
            chosen.stages.iter().map(|s| s.kind()).collect::<Vec<_>>()
        );
        chosen_device.insert(node_id, chosen.device);
        chosen_stages.insert(node_id, chosen.stages);
    }

    let mut exec = ExecGraph::new();
    let mut node_endpoints: HashMap<NodeId, (StageId, StageId)> =
        HashMap::with_capacity(topo.len());

    for node_id in &topo {
        let Some(stages) = chosen_stages.remove(node_id) else {
            continue;
        };
        if stages.is_empty() {
            continue;
        }
        let mut prev: Option<StageId> = None;
        let mut first: Option<StageId> = None;
        for stage in stages {
            let sid = exec.add_stage(stage);
            if first.is_none() {
                first = Some(sid);
            }
            if let Some(p) = prev {
                exec.add_edge(p, sid, ExecEdgePorts::default());
            }
            prev = Some(sid);
        }
        node_endpoints.insert(*node_id, (first.unwrap(), prev.unwrap()));
    }

    // Inter-node edges with device-transition bridges.
    for er in spec.graph.edge_references() {
        let Some(&(_, from_last)) = node_endpoints.get(&er.source()) else {
            continue;
        };
        let Some(&(to_first, _)) = node_endpoints.get(&er.target()) else {
            continue;
        };
        let from_dev = chosen_device
            .get(&er.source())
            .copied()
            .unwrap_or(Device::Cpu);
        let to_dev = chosen_device
            .get(&er.target())
            .copied()
            .unwrap_or(Device::Cpu);
        if from_dev == to_dev {
            exec.add_edge(from_last, to_first, ExecEdgePorts::default());
        } else {
            let bridge = match (from_dev, to_dev) {
                (Device::Cpu, Device::Gpu) => ExecNode::Upload(exec::Upload),
                (Device::Gpu, Device::Cpu) => ExecNode::Download(exec::Download),
                _ => unreachable!(),
            };
            tracing::info!(
                "[pixors] compile: insert bridge {:?}->{:?} ({})",
                from_dev,
                to_dev,
                bridge.kind()
            );
            let bid = exec.add_stage(bridge);
            exec.add_edge(from_last, bid, ExecEdgePorts::default());
            exec.add_edge(bid, to_first, ExecEdgePorts::default());
        }
    }

    exec.outputs = spec
        .outputs
        .iter()
        .filter_map(|(node, port)| {
            let &(_, last) = node_endpoints.get(node)?;
            Some((last, *port))
        })
        .collect();

    Ok(exec)
}

/// Pick the highest-scoring option. Score = `prefer` minus a heavy penalty
/// per predecessor whose chosen device differs (each such mismatch costs an
/// Upload/Download stage at runtime, so dominate `prefer`).
fn pick_option(options: Vec<ExpansionOption>, pred_devices: &[Device]) -> ExpansionOption {
    const TRANSITION_COST: i32 = 10;
    options
        .into_iter()
        .max_by_key(|opt| {
            let mismatches = pred_devices.iter().filter(|d| **d != opt.device).count() as i32;
            (opt.prefer as i32) - TRANSITION_COST * mismatches
        })
        .expect("non-empty options")
}
