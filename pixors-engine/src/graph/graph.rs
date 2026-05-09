use std::sync::Arc;

use petgraph::stable_graph::{NodeIndex, StableDiGraph};

use crate::stage::Stage;

pub type StageId = NodeIndex<u32>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EdgePorts {
    pub from_port: u16,
    pub to_port: u16,
}

pub type StageArc = Arc<dyn Stage + Send + Sync>;

pub struct ExecGraph {
    pub graph: StableDiGraph<StageArc, EdgePorts>,
    pub outputs: Vec<(StageId, u16)>,
}

impl ExecGraph {
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            outputs: vec![],
        }
    }

    pub fn add_stage(&mut self, stage: StageArc) -> StageId {
        self.graph.add_node(stage)
    }

    pub fn add_edge(&mut self, from: StageId, to: StageId, ports: EdgePorts) {
        self.graph.add_edge(from, to, ports);
    }

    pub fn stage_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn kind_names(&self) -> Vec<&'static str> {
        self.graph
            .node_indices()
            .map(|i| self.graph[i].kind())
            .collect()
    }
}
