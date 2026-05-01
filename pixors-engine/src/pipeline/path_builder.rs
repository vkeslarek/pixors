use std::marker::PhantomData;

use crate::pipeline::converter::Converter;
use crate::pipeline::graph::Graph;
use crate::pipeline::node::Node;
use crate::pipeline::operation::Operation;
use crate::pipeline::sink::Sink;
use crate::pipeline::source::Source;

pub struct PathBuilder<T> {
    nodes: Vec<Node>,
    edges: Vec<(usize, usize)>,
    last: Option<usize>,
    _marker: PhantomData<T>,
}

impl PathBuilder<()> {
    pub fn from_source<S: Source + serde::Serialize + 'static>(
        source: S,
    ) -> PathBuilder<S::Output> {
        let idx = 0;
        PathBuilder {
            nodes: vec![Node::from_source(source)],
            edges: Vec::new(),
            last: Some(idx),
            _marker: PhantomData,
        }
    }
}

impl<T: 'static> PathBuilder<T> {
    pub fn convert<C: Converter<Input = T> + serde::Serialize + 'static>(
        mut self,
        conv: C,
    ) -> PathBuilder<C::Output> {
        let idx = self.nodes.len();
        self.edges.push((self.last.unwrap(), idx));
        self.nodes.push(Node::from_converter(conv));
        PathBuilder {
            nodes: self.nodes,
            edges: self.edges,
            last: Some(idx),
            _marker: PhantomData,
        }
    }

    pub fn operation<O: Operation<Input = T> + serde::Serialize + 'static>(
        mut self,
        op: O,
    ) -> PathBuilder<O::Output> {
        let idx = self.nodes.len();
        self.edges.push((self.last.unwrap(), idx));
        self.nodes.push(Node::from_operation(op));
        PathBuilder {
            nodes: self.nodes,
            edges: self.edges,
            last: Some(idx),
            _marker: PhantomData,
        }
    }

    pub fn sink<Sk: Sink<Input = T> + serde::Serialize + 'static>(mut self, s: Sk) -> Graph {
        let idx = self.nodes.len();
        self.edges.push((self.last.unwrap(), idx));
        self.nodes.push(Node::from_sink(s));
        Graph {
            nodes: self.nodes,
            edges: self.edges,
            outputs: vec![idx],
        }
    }

    pub fn build(self) -> Graph {
        Graph {
            nodes: self.nodes,
            edges: self.edges,
            outputs: self.last.into_iter().collect(),
        }
    }

    pub fn split<const N: usize>(self) -> [PathBuilder<T>; N] {
        assert!(N > 0, "split requires N > 0");
        std::array::from_fn(|_| PathBuilder {
            nodes: self.nodes.iter().map(|n| n.clone_node()).collect(),
            edges: self.edges.clone(),
            last: self.last,
            _marker: PhantomData,
        })
    }
}
