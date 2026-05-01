use std::any::TypeId;

use crate::pipeline::node::{Node, NodeKind};

#[derive(Debug, Clone)]
pub struct TypeError {
    pub from: usize,
    pub to: usize,
    pub from_output: &'static str,
    pub to_input: &'static str,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<(usize, usize)>,
    pub outputs: Vec<usize>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            outputs: Vec::new(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn kinds(&self) -> Vec<NodeKind> {
        self.nodes.iter().map(|n| n.kind()).collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.nodes.iter().map(|n| n.name()).collect()
    }

    pub fn input_types(&self) -> Vec<Option<TypeId>> {
        self.nodes.iter().map(|n| n.input_type_id()).collect()
    }

    pub fn output_types(&self) -> Vec<Option<TypeId>> {
        self.nodes.iter().map(|n| n.output_type_id()).collect()
    }

    pub fn validate(&self) -> Result<(), Vec<TypeError>> {
        let mut errors = Vec::new();

        for &(from, to) in &self.edges {
            let out_id = self.nodes[from].output_type_id();
            let inp_id = self.nodes[to].input_type_id();

            match (out_id, inp_id) {
                (Some(out), Some(inp)) if out == inp => {}
                (Some(_), Some(_)) => {
                    errors.push(TypeError {
                        from,
                        to,
                        from_output: self.nodes[from]
                            .output_type_name()
                            .unwrap_or("<unknown>"),
                        to_input: self.nodes[to]
                            .input_type_name()
                            .unwrap_or("<unknown>"),
                    });
                }
                (None, _) => {
                    errors.push(TypeError {
                        from,
                        to,
                        from_output: "()",
                        to_input: self.nodes[to]
                            .input_type_name()
                            .unwrap_or("<unknown>"),
                    });
                }
                (_, None) => {
                    errors.push(TypeError {
                        from,
                        to,
                        from_output: self.nodes[from]
                            .output_type_name()
                            .unwrap_or("<unknown>"),
                        to_input: "()",
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
