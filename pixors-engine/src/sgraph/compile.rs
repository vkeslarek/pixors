use std::collections::HashMap;

use petgraph::visit::{EdgeRef, IntoEdgeReferences};

use crate::sgraph::cache::CacheIndex;
use crate::sgraph::graph::{NodeId, StateGraph};

use crate::egraph::graph::{ExecEdgePorts, ExecGraph, StageId};

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
    Apply,
    Preview { viewport_mip: u32 },
}

/// Lower a `StateGraph` into a flat, executable `ExecGraph`.
///
/// Steps:
///   1. Validate port types and confirm the graph is a DAG.
///   2. Walk the state graph in topological order; each `StateNode` expands
///      into a sequence of `ExecStage`s, chained intra-node by edges in the
///      output graph.
///   3. Inter-node edges from the state graph map to "last stage of `from`
///      → first stage of `to`" in the exec graph.
pub fn compile(
    spec: &StateGraph,
    _mode: ExecutionMode,
    _cache_index: &CacheIndex,
) -> Result<ExecGraph, CompileError> {
    spec.validate()
        .map_err(|e| CompileError::ValidationFailed(format!("{:?}", e)))?;
    let topo = spec
        .topological_order()
        .map_err(|_| CompileError::CycleDetected)?;

    let ctx = crate::sgraph::node::ExpandCtx;
    let mut exec = ExecGraph::new();
    // For each state node, the (first, last) stage it expanded into.
    let mut node_endpoints: HashMap<NodeId, (StageId, StageId)> =
        HashMap::with_capacity(topo.len());

    for node_id in topo {
        let stages = spec.graph[node_id].expand(&ctx);
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
        node_endpoints.insert(node_id, (first.unwrap(), prev.unwrap()));
    }

    // Inter-node edges.
    for er in spec.graph.edge_references() {
        let Some(&(_, from_last)) = node_endpoints.get(&er.source()) else {
            continue;
        };
        let Some(&(to_first, _)) = node_endpoints.get(&er.target()) else {
            continue;
        };
        exec.add_edge(from_last, to_first, ExecEdgePorts::default());
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
