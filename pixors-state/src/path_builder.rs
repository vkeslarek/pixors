use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pixors_engine::graph::graph::{EdgePorts, ExecGraph, StageArc, StageId};
use pixors_engine::graph::path::Path;

#[derive(Clone)]
struct Inner {
    stages: Vec<StageArc>,
    edges: Vec<(usize, usize)>,
    outputs: Vec<(usize, u16)>,
}

#[derive(Clone)]
pub struct PathBuilder {
    inner: Arc<Mutex<Inner>>,
    anchors: Vec<usize>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                stages: Vec::new(),
                edges: Vec::new(),
                outputs: Vec::new(),
            })),
            anchors: Vec::new(),
        }
    }

    pub fn src(self, s: StageArc) -> Self {
        self.add(s)
    }

    pub fn data_xform(self, d: StageArc) -> Self {
        self.add(d)
    }

    pub fn op(self, o: StageArc) -> Self {
        self.add(o)
    }

    pub fn sink(self, s: StageArc) -> Self {
        let next = self.add(s);
        {
            let mut inner = next.inner.lock().unwrap();
            inner.outputs.push((next.anchors[0], 0));
        }
        next
    }

    fn add(self, stage: StageArc) -> Self {
        let idx = {
            let mut inner = self.inner.lock().unwrap();
            let idx = inner.stages.len();
            inner.stages.push(stage);
            for &a in &self.anchors {
                inner.edges.push((a, idx));
            }
            idx
        };
        Self {
            anchors: vec![idx],
            ..self
        }
    }

    pub fn split<const N: usize>(self) -> [Self; N] {
        std::array::from_fn(|_| Self {
            inner: Arc::clone(&self.inner),
            anchors: self.anchors.clone(),
        })
    }

    pub fn attach(mut self, path: &Path) -> Self {
        for stage in path.stages() {
            self = self.add(Arc::clone(stage));
        }
        self
    }

    pub fn compile(self) -> ExecGraph {
        let inner = self.inner.lock().unwrap();
        let n = inner.stages.len();

        let mut seen: HashMap<usize, StageId> = HashMap::with_capacity(n);
        let mut remap: Vec<Option<StageId>> = vec![None; n];
        let mut graph = ExecGraph::new();

        for (i, stage) in inner.stages.iter().enumerate() {
            let key = Arc::as_ptr(stage) as *const () as usize;
            let sid = *seen
                .entry(key)
                .or_insert_with(|| graph.add_stage(Arc::clone(stage)));
            remap[i] = Some(sid);
        }

        for &(from, to) in &inner.edges {
            let f = remap[from].expect("from index");
            let t = remap[to].expect("to index");
            graph.add_edge(f, t, EdgePorts::default());
        }

        for &(stage, port) in &inner.outputs {
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
