use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use pixors_executor::data_transform::DataTransformNode;
use pixors_executor::graph::graph::{EdgePorts, ExecGraph, StageId};
use pixors_executor::operation::OperationNode;
use pixors_executor::sink::SinkNode;
use pixors_executor::source::SourceNode;
use pixors_executor::stage::StageNode;

#[derive(Clone)]
struct Inner {
    stages: Vec<StageNode>,
    edges: Vec<(usize, usize)>,
    outputs: Vec<(usize, u16)>,
}

#[derive(Clone)]
pub struct PathBuilder {
    inner: Rc<RefCell<Inner>>,
    anchors: Vec<usize>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner {
                stages: Vec::new(),
                edges: Vec::new(),
                outputs: Vec::new(),
            })),
            anchors: Vec::new(),
        }
    }

    pub fn src(self, s: impl Into<SourceNode>) -> Self {
        self.add(StageNode::Source(s.into()))
    }

    pub fn data_xform(self, d: impl Into<DataTransformNode>) -> Self {
        self.add(StageNode::DataTransform(d.into()))
    }

    pub fn op(self, o: impl Into<OperationNode>) -> Self {
        self.add(StageNode::Operation(o.into()))
    }

    pub fn sink(self, s: impl Into<SinkNode>) -> Self {
        self.add(StageNode::Sink(s.into()))
    }

    fn add(self, stage: StageNode) -> Self {
        let idx = {
            let mut inner = self.inner.borrow_mut();
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

    pub fn mark_output(self, port: u16) -> Self {
        {
            let mut inner = self.inner.borrow_mut();
            for &a in &self.anchors {
                inner.outputs.push((a, port));
            }
        }
        self
    }

    pub fn split<const N: usize>(self) -> [Self; N] {
        std::array::from_fn(|_| Self {
            inner: Rc::clone(&self.inner),
            anchors: self.anchors.clone(),
        })
    }

    pub fn compile(self) -> ExecGraph {
        let inner = self.inner.borrow();
        let n = inner.stages.len();

        let mut seen: HashMap<String, StageId> = HashMap::with_capacity(n);
        let mut remap: Vec<Option<StageId>> = vec![None; n];
        let mut graph = ExecGraph::new();

        for (i, stage) in inner.stages.iter().enumerate() {
            let key = format!("{:?}", stage);
            let sid = *seen.entry(key).or_insert_with(|| {
                graph.add_stage(stage.clone())
            });
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
