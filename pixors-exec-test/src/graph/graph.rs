use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use serde::{Deserialize, Serialize};

use crate::stage::{StageNode, Stage};

/// Stable handle to a stage in an `ExecGraph`.
pub type StageId = NodeIndex<u32>;

/// Edge weight: which output port of the source stage feeds which input port
/// of the destination stage. All current stages use port 0; the field stays
/// for forward-compatibility with multi-output stages (e.g. tee, channel
/// split).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgePorts {
    pub from_port: u16,
    pub to_port: u16,
}

/// Compiled, ready-to-run pipeline produced by `state_graph::compile`.
///
/// Backed by `petgraph::StableDiGraph`: handles to stages stay valid across
/// rewrites and the executor can run topological sort + traversal directly
/// on the graph.
pub struct ExecGraph {
    pub graph: StableDiGraph<StageNode, EdgePorts>,
    pub outputs: Vec<(StageId, u16)>,
}

impl ExecGraph {
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            outputs: vec![],
        }
    }

    pub fn add_stage(&mut self, stage: StageNode) -> StageId {
        self.graph.add_node(stage)
    }

    pub fn add_edge(&mut self, from: StageId, to: StageId, ports: EdgePorts) {
        self.graph.add_edge(from, to, ports);
    }

    pub fn stage_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Variant kinds for every stage, in iteration order.
    pub fn kind_names(&self) -> Vec<&'static str> {
        self.graph
            .node_indices()
            .map(|i| self.graph[i].kind())
            .collect()
    }
}
