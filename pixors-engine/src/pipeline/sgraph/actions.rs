use serde::{Deserialize, Serialize};

use crate::pipeline::sgraph::graph::{EdgePorts, NodeId};
use crate::pipeline::sgraph::node::StateNode;

/// Snapshot captured when a node is removed, sufficient to restore it on
/// undo (the node payload plus every incident edge and any output entries
/// that referenced it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovedNode {
    pub node: StateNode,
    pub in_edges: Vec<(NodeId, EdgePorts)>,
    pub out_edges: Vec<(NodeId, EdgePorts)>,
    pub output_ports: Vec<u16>,
}

/// Single mutation applied to a `StateGraph`.
///
/// `History::push` mutates each variant in place after applying it, filling
/// in the metadata (`assigned`, `snapshot`, `prev`) needed by `undo`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    InsertNode {
        node: StateNode,
        /// NodeId allocated by `add_node`; filled in after apply.
        assigned: Option<NodeId>,
    },
    RemoveNode {
        id: NodeId,
        /// Captured *before* removal so undo can restore edges and outputs.
        snapshot: Option<RemovedNode>,
    },
    Connect {
        from: NodeId,
        to: NodeId,
        ports: EdgePorts,
    },
    Disconnect {
        from: NodeId,
        to: NodeId,
        ports: EdgePorts,
    },
    UpdateParams {
        id: NodeId,
        params: serde_json::Value,
        /// Previous params, captured before apply so undo can restore them.
        prev: Option<serde_json::Value>,
    },
}

/// Group of actions applied — and undone — atomically.
pub type ActionBatch = Vec<Action>;
