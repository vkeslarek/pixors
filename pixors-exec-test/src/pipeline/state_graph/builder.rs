use crate::debug_stopwatch;
use crate::pipeline::exec_graph::executor::Executor;
use crate::error::Error;
use crate::pipeline::state_graph::cache::CacheIndex;
use crate::pipeline::state_graph::compile::{compile, ExecutionMode};
use crate::pipeline::state_graph::graph::{EdgePorts, NodeId, StateGraph};
use crate::pipeline::state::StateNode;

/// Linear pipeline builder: chains source → operations → sink in insertion
/// order. Each `pipe`/`sink` connects the last-added node to the new one via
/// port 0 → 0.
pub struct PathBuilder {
    graph: StateGraph,
    last: Option<NodeId>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            graph: StateGraph::new(),
            last: None,
        }
    }

    pub fn source(mut self, node: StateNode) -> Self {
        self.last = Some(self.graph.add_node(node));
        self
    }

    pub fn operation(mut self, node: StateNode) -> Self {
        let id = self.graph.add_node(node);
        if let Some(prev) = self.last {
            self.graph.add_edge(prev, id, EdgePorts::default());
        }
        self.last = Some(id);
        self
    }

    pub fn pipe(self, node: StateNode) -> Self {
        self.operation(node)
    }

    pub fn sink(mut self, node: StateNode) -> Self {
        let id = self.graph.add_node(node);
        if let Some(prev) = self.last {
            self.graph.add_edge(prev, id, EdgePorts::default());
        }
        self.graph.outputs = vec![(id, 0)];
        self.last = Some(id);
        self
    }

    pub fn into_graph(self) -> StateGraph {
        self.graph
    }

    pub fn run(self, mode: ExecutionMode) -> Result<(), Error> {
        let _sw = debug_stopwatch!("pipeline");
        self.graph
            .validate()
            .map_err(|e| Error::internal(format!("{:?}", e)))?;
        let ci = CacheIndex::new();
        let gpu_available = if mode.force_cpu() {
            false
        } else {
            crate::gpu::gpu_available()
        };

        let exec = compile(&self.graph, mode, &ci, gpu_available)
            .map_err(|e| Error::internal(format!("{:?}", e)))?;
        let mut executor = Executor::new(&exec)?;
        executor.run()
    }
}
