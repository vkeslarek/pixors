use petgraph::Direction;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use serde::{Deserialize, Serialize};

use crate::pipeline::sgraph::actions::{Action, ActionBatch, RemovedNode};
use crate::pipeline::sgraph::graph::{NodeId, StateGraph};
use crate::pipeline::sgraph::node::StateNode;

/// Bounded undo/redo log over `ActionBatch`es.
///
/// `push(batch)` mutates the graph and rewrites each action in place to
/// capture the state needed for inversion (assigned NodeIds, removal
/// snapshots, previous params). `undo` then walks the batch in reverse and
/// reverts each action using that captured state.
///
/// **Caveat:** undoing a `RemoveNode` re-inserts the node with a *new*
/// `NodeId` (petgraph's `StableDiGraph` allocates indices monotonically and
/// cannot reuse a freed slot at-will). Within a single batch, if a later
/// action references the removed node, its inverse will dangle. Keep batches
/// independent (one structural change per batch) to avoid this.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    past: Vec<ActionBatch>,
    future: Vec<ActionBatch>,
    max_depth: usize,
}

impl History {
    pub fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            ..Self::default()
        }
    }

    pub fn push(&mut self, mut batch: ActionBatch, graph: &mut StateGraph) {
        // A new edit invalidates redo: branching from the past throws away
        // the previously-undone future.
        self.future.clear();

        for action in batch.iter_mut() {
            apply(action, graph);
        }
        graph.version += 1;

        self.past.push(batch);
        if self.past.len() > self.max_depth {
            self.past.remove(0);
        }
    }

    pub fn undo(&mut self, graph: &mut StateGraph) -> bool {
        let Some(batch) = self.past.pop() else {
            return false;
        };
        for action in batch.iter().rev() {
            revert(action, graph);
        }
        graph.version += 1;
        self.future.push(batch);
        true
    }

    pub fn redo(&mut self, graph: &mut StateGraph) -> bool {
        let Some(mut batch) = self.future.pop() else {
            return false;
        };
        for action in batch.iter_mut() {
            apply(action, graph);
        }
        graph.version += 1;
        self.past.push(batch);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

/// Apply `action` forward, populating any post-apply metadata.
fn apply(action: &mut Action, graph: &mut StateGraph) {
    match action {
        Action::InsertNode { node, assigned } => {
            *assigned = Some(graph.add_node(node.clone()));
        }
        Action::RemoveNode { id, snapshot } => {
            *snapshot = Some(capture_snapshot(graph, *id));
            graph.outputs.retain(|(n, _)| n != id);
            graph.graph.remove_node(*id);
        }
        Action::Connect { from, to, ports } => {
            graph.add_edge(*from, *to, *ports);
        }
        Action::Disconnect { from, to, ports } => {
            remove_edge(graph, *from, *to, *ports);
        }
        Action::UpdateParams { id, params, prev } => {
            let Some(node) = graph.node_mut(*id) else {
                return;
            };
            *prev = Some(node.serialize_params());
            if let Ok(parsed) = serde_json::from_value::<StateNode>(params.clone()) {
                *node = parsed;
            }
        }
    }
}

/// Reverse a previously-applied action using the metadata captured at apply
/// time. Missing metadata silently no-ops — that means the action was never
/// applied or was applied by a different code path.
fn revert(action: &Action, graph: &mut StateGraph) {
    match action {
        Action::InsertNode {
            assigned: Some(id), ..
        } => {
            graph.outputs.retain(|(n, _)| n != id);
            graph.graph.remove_node(*id);
        }
        Action::RemoveNode {
            snapshot: Some(snap),
            ..
        } => {
            restore_snapshot(graph, snap);
        }
        Action::Connect { from, to, ports } => {
            remove_edge(graph, *from, *to, *ports);
        }
        Action::Disconnect { from, to, ports } => {
            graph.add_edge(*from, *to, *ports);
        }
        Action::UpdateParams {
            id,
            prev: Some(prev),
            ..
        } => {
            let Some(node) = graph.node_mut(*id) else {
                return;
            };
            if let Ok(parsed) = serde_json::from_value::<StateNode>(prev.clone()) {
                *node = parsed;
            }
        }
        _ => {}
    }
}

fn capture_snapshot(graph: &StateGraph, id: NodeId) -> RemovedNode {
    RemovedNode {
        node: graph.graph[id].clone(),
        in_edges: graph
            .graph
            .edges_directed(id, Direction::Incoming)
            .map(|er| (er.source(), *er.weight()))
            .collect(),
        out_edges: graph
            .graph
            .edges_directed(id, Direction::Outgoing)
            .map(|er| (er.target(), *er.weight()))
            .collect(),
        output_ports: graph
            .outputs
            .iter()
            .filter_map(|(n, p)| (n == &id).then_some(*p))
            .collect(),
    }
}

fn restore_snapshot(graph: &mut StateGraph, snap: &RemovedNode) {
    let new_id = graph.add_node(snap.node.clone());
    for (src, ports) in &snap.in_edges {
        if graph.node(*src).is_some() {
            graph.add_edge(*src, new_id, *ports);
        }
    }
    for (tgt, ports) in &snap.out_edges {
        if graph.node(*tgt).is_some() {
            graph.add_edge(new_id, *tgt, *ports);
        }
    }
    for port in &snap.output_ports {
        graph.outputs.push((new_id, *port));
    }
}

fn remove_edge(
    graph: &mut StateGraph,
    from: NodeId,
    to: NodeId,
    ports: crate::pipeline::sgraph::graph::EdgePorts,
) {
    let edge_id = graph
        .graph
        .edge_references()
        .find(|er| er.source() == from && er.target() == to && *er.weight() == ports)
        .map(|er| er.id());
    if let Some(eid) = edge_id {
        graph.graph.remove_edge(eid);
    }
}
