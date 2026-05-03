use std::collections::{HashMap, HashSet};

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::visit::EdgeRef;

use crate::data::Device;
use crate::stage::{ExecNode, Stage};
use crate::graph::graph::{ExecEdgePorts, ExecGraph, StageId};

/// Walk the ExecGraph and fuse runs of adjacent GPU nodes into
/// FusedGpuKernel nodes. Returns a new graph.
pub fn fuse_gpu_kernels(graph: &ExecGraph) -> ExecGraph {
    let chains = find_gpu_chains(graph);
    if chains.iter().all(|c| c.len() < 2) {
        tracing::info!("[pixors] fusion: no GPU chains of length >= 2, skipping");
        return rebuild_unchanged(graph);
    }
    tracing::info!(
        "[pixors] fusion: found {} GPU chain(s): {:?}",
        chains.len(),
        chains.iter().map(|c| c.len()).collect::<Vec<_>>()
    );

    let mut in_chain: HashMap<StageId, (usize, usize)> = HashMap::new();
    for (ci, chain) in chains.iter().enumerate() {
        for (pos, &sid) in chain.iter().enumerate() {
            in_chain.insert(sid, (ci, pos));
        }
    }

    let mut new_graph = ExecGraph::new();
    let mut id_map: HashMap<StageId, StageId> = HashMap::new();

    let topo = toposort(&graph.graph, None).expect("no cycle");

    for &old_id in &topo {
        let node = &graph.graph[old_id];

        if let Some(&(ci, pos)) = in_chain.get(&old_id) {
            let chain = &chains[ci];
            if chain.len() < 2 {
                let new_id = new_graph.add_stage(node.clone());
                id_map.insert(old_id, new_id);
            } else if pos == 0 {
                let steps: Vec<ExecNode> = chain
                    .iter()
                    .map(|&sid| graph.graph[sid].clone())
                    .collect();
                let fused =
                    crate::operation::FusedGpuKernel { steps };
                let new_id =
                    new_graph.add_stage(ExecNode::FusedGpuKernel(fused));
                for &sid in chain {
                    id_map.insert(sid, new_id);
                }
            }
        } else {
            let new_id = new_graph.add_stage(node.clone());
            id_map.insert(old_id, new_id);
        }
    }

    let mut added_edges: HashSet<(StageId, StageId)> = HashSet::new();
    for &old_id in &topo {
        for er in graph.graph.edges_directed(old_id, Direction::Outgoing) {
            let src = id_map[&old_id];
            let tgt = id_map[&er.target()];
            if src != tgt && added_edges.insert((src, tgt)) {
                new_graph.add_edge(src, tgt, ExecEdgePorts::default());
            }
        }
    }

    new_graph.outputs = graph
        .outputs
        .iter()
        .filter_map(|(old_id, port)| Some((*id_map.get(old_id)?, *port)))
        .collect();

    new_graph
}

fn find_gpu_chains(graph: &ExecGraph) -> Vec<Vec<StageId>> {
    let topo = toposort(&graph.graph, None).expect("no cycle");
    let mut visited: HashSet<StageId> = HashSet::new();
    let mut chains: Vec<Vec<StageId>> = Vec::new();

    for &sid in &topo {
        if visited.contains(&sid) {
            continue;
        }
        if !is_gpu_node(graph, sid) {
            continue;
        }

        let pred_is_gpu = graph
            .graph
            .edges_directed(sid, Direction::Incoming)
            .any(|er| is_gpu_node(graph, er.source()));
        if pred_is_gpu {
            continue;
        }

        let mut chain = vec![sid];
        visited.insert(sid);
        let mut cur = sid;

        loop {
            let succs: Vec<StageId> = graph
                .graph
                .edges_directed(cur, Direction::Outgoing)
                .map(|er| er.target())
                .collect();
            if succs.len() != 1 {
                break;
            }
            let next = succs[0];
            if !is_gpu_node(graph, next) {
                break;
            }
            let preds: Vec<_> = graph
                .graph
                .edges_directed(next, Direction::Incoming)
                .collect();
            if preds.len() != 1 {
                break;
            }
            chain.push(next);
            visited.insert(next);
            cur = next;
        }
        chains.push(chain);
    }
    chains
}

fn is_gpu_node(graph: &ExecGraph, sid: StageId) -> bool {
    graph.graph[sid].device() == Device::Gpu
}

fn rebuild_unchanged(graph: &ExecGraph) -> ExecGraph {
    let topo = toposort(&graph.graph, None).expect("no cycle");
    let mut new_graph = ExecGraph::new();
    let mut id_map: HashMap<StageId, StageId> = HashMap::new();

    for &old_id in &topo {
        let new_id = new_graph.add_stage(graph.graph[old_id].clone());
        id_map.insert(old_id, new_id);
    }
    for &old_id in &topo {
        for er in graph.graph.edges_directed(old_id, Direction::Outgoing) {
            new_graph.add_edge(
                id_map[&old_id],
                id_map[&er.target()],
                ExecEdgePorts::default(),
            );
        }
    }
    new_graph.outputs = graph
        .outputs
        .iter()
        .filter_map(|(old_id, port)| Some((*id_map.get(old_id)?, *port)))
        .collect();
    new_graph
}
