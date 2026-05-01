use crate::pipeline::converter::AnyConverter;
use crate::pipeline::operation::AnyOperation;
use crate::pipeline::sink::AnySink;
use crate::pipeline::source::AnySource;
use std::any::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Source,
    Sink,
    Operation,
    Converter,
}

pub enum Node {
    Source(Box<dyn AnySource>),
    Sink(Box<dyn AnySink>),
    Operation(Box<dyn AnyOperation>),
    Converter(Box<dyn AnyConverter>),
}

impl Clone for Node {
    fn clone(&self) -> Self {
        self.clone_node()
    }
}

impl Node {
    pub fn from_source<S: crate::pipeline::source::Source + serde::Serialize + 'static>(s: S) -> Self {
        Node::Source(Box::new(s))
    }

    pub fn from_sink<S: crate::pipeline::sink::Sink + serde::Serialize + 'static>(s: S) -> Self {
        Node::Sink(Box::new(s))
    }

    pub fn from_operation<O: crate::pipeline::operation::Operation + serde::Serialize + 'static>(
        o: O,
    ) -> Self {
        Node::Operation(Box::new(o))
    }

    pub fn from_converter<C: crate::pipeline::converter::Converter + serde::Serialize + 'static>(
        c: C,
    ) -> Self {
        Node::Converter(Box::new(c))
    }

    pub fn kind(&self) -> NodeKind {
        match self {
            Node::Source(_) => NodeKind::Source,
            Node::Sink(_) => NodeKind::Sink,
            Node::Operation(_) => NodeKind::Operation,
            Node::Converter(_) => NodeKind::Converter,
        }
    }

    pub fn params(&self) -> serde_json::Value {
        match self {
            Node::Source(s) => s.params(),
            Node::Sink(s) => s.params(),
            Node::Operation(o) => o.params(),
            Node::Converter(c) => c.params(),
        }
    }

    pub fn input_type_id(&self) -> Option<TypeId> {
        match self {
            Node::Source(_) => None,
            Node::Sink(sink) => Some(sink.input_type_id()),
            Node::Operation(operation) => Some(operation.input_type_id()),
            Node::Converter(converter) => Some(converter.input_type_id()),
        }
    }

    pub fn output_type_id(&self) -> Option<TypeId> {
        match self {
            Node::Source(source) => Some(source.output_type_id()),
            Node::Sink(_) => None,
            Node::Operation(operation) => Some(operation.output_type_id()),
            Node::Converter(converter) => Some(converter.output_type_id()),
        }
    }

    pub fn input_type_name(&self) -> Option<&'static str> {
        match self {
            Node::Source(_) => None,
            Node::Sink(sink) => Some(sink.input_type_name()),
            Node::Operation(operation) => Some(operation.input_type_name()),
            Node::Converter(converter) => Some(converter.input_type_name()),
        }
    }

    pub fn output_type_name(&self) -> Option<&'static str> {
        match self {
            Node::Source(source) => Some(source.output_type_name()),
            Node::Sink(_) => None,
            Node::Operation(operation) => Some(operation.output_type_name()),
            Node::Converter(converter) => Some(converter.output_type_name()),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Node::Source(souce) => souce.name(),
            Node::Sink(sink) => sink.name(),
            Node::Operation(operation) => operation.name(),
            Node::Converter(converter) => converter.name(),
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Node::Source(_) => "Source",
            Node::Sink(_) => "Sink",
            Node::Operation(_) => "Operation",
            Node::Converter(_) => "Converter",
        }
    }

    pub fn clone_node(&self) -> Self {
        match self {
            Node::Source(s) => Node::Source(s.clone_source()),
            Node::Sink(s) => Node::Sink(s.clone_sink()),
            Node::Operation(o) => Node::Operation(o.clone_operation()),
            Node::Converter(c) => Node::Converter(c.clone_converter()),
        }
    }
}
