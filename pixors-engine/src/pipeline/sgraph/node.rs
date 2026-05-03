use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use crate::pipeline::egraph::stage::{Device, ExecStage};
use crate::pipeline::state::*;
use crate::pipeline::sgraph::ports::PortSpec;

/// Per-expansion context. Hands the compiler hints about runtime
/// capabilities so nodes can produce viable expansion options.
pub struct ExpandCtx {
    pub gpu_available: bool,
}

impl ExpandCtx {
    pub fn cpu_only() -> Self {
        Self { gpu_available: false }
    }
}

/// One viable lowering of a `StateNode` into a fixed sequence of
/// `ExecStage`s that all run on the same `Device`. A node may publish
/// several options; the compiler picks one to minimize CPU↔GPU transitions.
pub struct ExpansionOption {
    pub stages: Vec<ExecStage>,
    pub device: Device,
    /// Higher = preferred when ties allow. Used to favor GPU when no
    /// transition cost dominates.
    pub prefer: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

#[enum_dispatch]
pub trait StateNodeTrait {
    fn kind(&self) -> &'static str;
    fn inputs(&self) -> Vec<PortSpec>;
    fn outputs(&self) -> Vec<PortSpec>;
    fn expand(&self, ctx: &ExpandCtx) -> Vec<ExpansionOption>;
}

/// User-facing node in the state graph (`sgraph`).
///
/// A `StateNode` is a high-level operation. Compilation calls `expand` to turn
/// each one into a sequence of low-level `ExecStage`s that the runner executes.
#[enum_dispatch(StateNodeTrait)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateNode {
    FileImage,
    Blur,
    DiskCache,
    DisplayCache,
    Export,
}

impl StateNode {
    pub fn serialize_params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}
