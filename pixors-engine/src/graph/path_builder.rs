use crate::graph::graph::{EdgePorts, ExecGraph, StageId};
use crate::stage::Stage;

pub struct PathBuilder {
    stages: Vec<Stage>,
    edges: Vec<(usize, usize)>,
    outputs: Vec<(usize, u16)>,
    anchors: Vec<usize>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            edges: Vec::new(),
            outputs: Vec::new(),
            anchors: Vec::new(),
        }
    }

    pub fn src(self, s: Stage) -> Self {
        self.add(s)
    }
    pub fn data_xform(self, d: Stage) -> Self {
        self.add(d)
    }
    pub fn op(self, o: Stage) -> Self {
        self.add(o)
    }

    pub fn sink(self, s: Stage) -> Self {
        let mut next = self.add(s);
        let last = *next.anchors.last().expect("anchor after add");
        next.outputs.push((last, 0));
        next
    }

    fn add(mut self, stage: Stage) -> Self {
        let idx = self.stages.len();
        self.stages.push(stage);
        for &a in &self.anchors {
            self.edges.push((a, idx));
        }
        self.anchors = vec![idx];
        self
    }

    pub fn compile(self) -> ExecGraph {
        let mut remap: Vec<Option<StageId>> = vec![None; self.stages.len()];
        let mut graph = ExecGraph::new();
        for (i, stage) in self.stages.into_iter().enumerate() {
            remap[i] = Some(graph.add_stage(stage));
        }
        for &(from, to) in &self.edges {
            let f = remap[from].expect("from index");
            let t = remap[to].expect("to index");
            graph.add_edge(f, t, EdgePorts::default());
        }
        for &(stage, port) in &self.outputs {
            let s = remap[stage].expect("output stage index");
            graph.outputs.push((s, port));
        }
        graph
    }
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}
