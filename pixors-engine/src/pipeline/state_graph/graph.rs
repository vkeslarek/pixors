use petgraph::algo::toposort;
use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableDiGraph};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use serde::{Deserialize, Serialize};

use crate::pipeline::state::{StateNode, StateNodeTrait};
use crate::pipeline::state_graph::ports::PortType;

/// Stable handle to a node in the `StateGraph`. Stays valid across removals
/// of *other* nodes — the index is recycled only after the node it points
/// to is removed and a new one is inserted.
pub type NodeId = NodeIndex<u32>;

/// Stable handle to an edge in the `StateGraph`.
pub type EdgeId = EdgeIndex<u32>;

/// Edge weight: which output port feeds which input port.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgePorts {
    pub from_port: u16,
    pub to_port: u16,
}

impl EdgePorts {
    pub fn new(from_port: u16, to_port: u16) -> Self {
        Self { from_port, to_port }
    }
}

/// User-editable graph of operations. The "source of truth" the UI mutates;
/// compiled into an `ExecGraph` for execution.
///
/// Backed by `petgraph::StableDiGraph`: node and edge handles remain valid
/// across removals of unrelated entities, so callers (undo/redo, caches,
/// renderers) can store handles without re-resolving them on every mutation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateGraph {
    pub graph: StableDiGraph<StateNode, EdgePorts>,
    /// `(node, port)` pairs the runner should expose as final outputs.
    pub outputs: Vec<(NodeId, u16)>,
    /// Monotonically increasing counter, bumped whenever the graph changes.
    /// Cheap dirty-check for caches, UI subscribers, etc.
    pub version: u64,
}

#[derive(Debug, Clone)]
pub struct TypeError {
    pub edge: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub from_type: PortType,
    pub to_type: PortType,
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    TypeMismatches(Vec<TypeError>),
    Cycle,
}

impl StateGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Insert a node and return its stable handle.
    pub fn add_node(&mut self, node: StateNode) -> NodeId {
        self.graph.add_node(node)
    }

    /// Connect two nodes by their port indices.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, ports: EdgePorts) -> EdgeId {
        self.graph.add_edge(from, to, ports)
    }

    pub fn node(&self, id: NodeId) -> Option<&StateNode> {
        self.graph.node_weight(id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut StateNode> {
        self.graph.node_weight_mut(id)
    }

    /// Check every edge for port-type compatibility.
    ///
    /// `Unit` ports act as wildcards (sinks) and are always accepted. Edges
    /// referencing missing ports are silently skipped here — that is a
    /// structural error the caller should catch separately.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut errors = vec![];

        for edge_ref in self.graph.edge_references() {
            let from = edge_ref.source();
            let to = edge_ref.target();
            let ports = edge_ref.weight();

            let from_type = self.graph[from]
                .outputs()
                .get(ports.from_port as usize)
                .map(|p| p.port_type);
            let to_type = self.graph[to]
                .inputs()
                .get(ports.to_port as usize)
                .map(|p| p.port_type);

            let (Some(ft), Some(tt)) = (from_type, to_type) else {
                continue;
            };
            if ft == PortType::Unit || tt == PortType::Unit {
                continue;
            }
            if ft != tt {
                errors.push(TypeError {
                    edge: edge_ref.id(),
                    from,
                    to,
                    from_type: ft,
                    to_type: tt,
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::TypeMismatches(errors))
        }
    }

    /// Returns nodes in execution order, or `Cycle` if the graph is not a DAG.
    pub fn topological_order(&self) -> Result<Vec<NodeId>, ValidationError> {
        toposort(&self.graph, None).map_err(|_| ValidationError::Cycle)
    }
}
