//! Runtime stage system — traits for pipeline actors and graph-level metadata.
//!
//! - `actors` — Producer (sources), Processor (transforms), Consumer (sinks)
//! - `kinds`  — DataKind, PortDeclaration, PortGroup, PortSpecification
//! - `context`— ProcessorContext passed to every runtime invocation
//! - `node`   — Stage trait (factory), StageNode enum, delegate_stage! macro

mod actors;
mod context;
mod kinds;
mod node;

pub use actors::{Consumer, Producer, Processor};
pub use context::ProcessorContext;
pub use kinds::{DataKind, PortDeclaration, PortGroup, PortSpecification};
pub use node::{Stage, StageNode, StageRole};
