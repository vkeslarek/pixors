use crate::data::device::Device;
use crate::stage::actors::{Consumer, Producer, Processor};
use crate::stage::Stage;

pub type StageId = usize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EdgePorts {
    pub from_port: u16,
    pub to_port: u16,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: StageId,
    pub to: StageId,
    pub ports: EdgePorts,
}

pub struct ChainData {
    pub producer: Option<Box<dyn Producer>>,
    pub processors: Vec<Box<dyn Processor>>,
    pub consumer: Option<Box<dyn Consumer>>,
    pub device: Device,
}

pub enum Node {
    Stage(Stage),
    Chain(ChainData),
    Dead,
}

pub struct ExecGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub outputs: Vec<(StageId, u16)>,
}

impl ExecGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new(), outputs: Vec::new() }
    }

    pub fn add_stage(&mut self, stage: impl Into<Node>) -> StageId {
        let id = self.nodes.len();
        self.nodes.push(stage.into());
        id
    }

    pub fn add_edge(&mut self, from: StageId, to: StageId, ports: EdgePorts) {
        self.edges.push(Edge { from, to, ports });
    }

    pub fn stage(&self, id: StageId) -> &Stage {
        match &self.nodes[id] {
            Node::Stage(s) => s,
            _ => panic!("not a stage"),
        }
    }

    pub fn stage_mut(&mut self, id: StageId) -> &mut Stage {
        match &mut self.nodes[id] {
            Node::Stage(s) => s,
            _ => panic!("not a stage"),
        }
    }

    pub fn take_stage(&mut self, id: StageId) -> Stage {
        match std::mem::replace(&mut self.nodes[id], Node::Dead) {
            Node::Stage(s) => s,
            _ => panic!("not a stage"),
        }
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }

    pub fn remove_edge(&mut self, edge_idx: usize) { self.edges.remove(edge_idx); }

    pub fn find_edge(&self, from: StageId, to: StageId) -> Option<usize> {
        self.edges.iter().position(|e| e.from == from && e.to == to)
    }

    pub fn edges_out(&self, id: StageId) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.from == id)
    }

    pub fn edges_in(&self, id: StageId) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.to == id)
    }

    pub fn edge_indices(&self) -> impl Iterator<Item = usize> { 0..self.edges.len() }

    pub fn edge_endpoints(&self, ei: usize) -> Option<(StageId, StageId)> {
        self.edges.get(ei).map(|e| (e.from, e.to))
    }

    pub fn edge_weight(&self, ei: usize) -> Option<&EdgePorts> {
        self.edges.get(ei).map(|e| &e.ports)
    }

    pub fn edge_weight_mut(&mut self, ei: usize) -> Option<&mut EdgePorts> {
        self.edges.get_mut(ei).map(|e| &mut e.ports)
    }

    pub fn node_kind(&self, id: StageId) -> &'static str {
        match &self.nodes[id] {
            Node::Stage(s) => s.kind(),
            Node::Chain(_) => "chain",
            Node::Dead => "dead",
        }
    }

    pub fn kind_names(&self) -> Vec<&'static str> {
        self.nodes.iter().map(|n| match n {
            Node::Stage(s) => s.kind(),
            Node::Chain(_) => "chain",
            Node::Dead => "dead",
        }).collect()
    }

    pub fn toposort(&self) -> Result<Vec<StageId>, String> {
        let n = self.nodes.len();
        let mut in_degree = vec![0usize; n];
        for e in &self.edges {
            in_degree[e.to] += 1;
        }
        let mut queue: Vec<StageId> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        let mut qi = 0;
        while qi < queue.len() {
            let u = queue[qi]; qi += 1;
            order.push(u);
            for e in self.edges_out(u) {
                in_degree[e.to] -= 1;
                if in_degree[e.to] == 0 { queue.push(e.to); }
            }
        }
        if order.len() != n { return Err("pipeline graph has a cycle".into()); }
        Ok(order)
    }
}

impl From<Stage> for Node {
    fn from(s: Stage) -> Self { Node::Stage(s) }
}
