use crate::data::device::Device;
use crate::data_transform::DataTransformNode;
use crate::operation::OperationNode;
use crate::sink::SinkNode;
use crate::source::SourceNode;
use serde::{Deserialize, Serialize};

use super::actors::{Consumer, Processor, Producer};
use super::kinds::PortSpecification;

// ── StageHints ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StageHints {
    pub device: Device,
    pub preference: Option<Device>,
}

impl Default for StageHints {
    fn default() -> Self {
        Self {
            device: Device::Cpu,
            preference: None,
        }
    }
}

impl StageHints {
    pub const fn cpu() -> Self {
        Self {
            device: Device::Cpu,
            preference: None,
        }
    }

    pub const fn gpu() -> Self {
        Self {
            device: Device::Gpu,
            preference: None,
        }
    }

    pub const fn either() -> Self {
        Self {
            device: Device::Either,
            preference: None,
        }
    }

    pub const fn prefer_cpu() -> Self {
        Self {
            device: Device::Either,
            preference: Some(Device::Cpu),
        }
    }

    pub const fn prefer_gpu() -> Self {
        Self {
            device: Device::Either,
            preference: Some(Device::Gpu),
        }
    }
}

// ── Stage trait ────────────────────────────────────────────────────────────────

pub trait Stage {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpecification;
    fn hints(&self) -> StageHints {
        StageHints::cpu()
    }
    fn producer(&self) -> Option<Box<dyn Producer>> {
        None
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        None
    }
    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        None
    }
    /// How many items this stage emits per item received (1.0 = passthrough).
    fn work_multiplier(&self) -> f64 {
        1.0
    }
    /// How many items a source stage emits total (0 for non-sources).
    fn source_items(&self) -> usize {
        0
    }
}

// ── Enum dispatch macro ────────────────────────────────────────────────────────

#[macro_export]
macro_rules! delegate_stage {
    ($enum:ty, $($variant:ident),+ $(,)?) => {
        impl $crate::stage::Stage for $enum {
            fn kind(&self) -> &'static str {
                match self { $(Self::$variant(n) => n.kind()),+ }
            }
            fn ports(&self) -> &'static $crate::stage::PortSpecification {
                match self { $(Self::$variant(n) => n.ports()),+ }
            }
            fn hints(&self) -> $crate::stage::StageHints {
                match self { $(Self::$variant(n) => n.hints()),+ }
            }
            fn producer(&self) -> Option<Box<dyn $crate::stage::Producer>> {
                match self { $(Self::$variant(n) => n.producer()),+ }
            }
            fn processor(&self) -> Option<Box<dyn $crate::stage::Processor>> {
                match self { $(Self::$variant(n) => n.processor()),+ }
            }
            fn consumer(&self) -> Option<Box<dyn $crate::stage::Consumer>> {
                match self { $(Self::$variant(n) => n.consumer()),+ }
            }
            fn work_multiplier(&self) -> f64 {
                match self { $(Self::$variant(n) => n.work_multiplier()),+ }
            }
            fn source_items(&self) -> usize {
                match self { $(Self::$variant(n) => n.source_items()),+ }
            }
        }
    };
}

delegate_stage!(StageNode, Source, Sink, Operation, DataTransform);

// ── StageNode: serialisable wrapper ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StageNode {
    Source(SourceNode),
    Sink(SinkNode),
    Operation(OperationNode),
    DataTransform(DataTransformNode),
}

impl StageNode {
    pub fn role(&self) -> StageRole {
        let ports = self.ports();
        match (ports.inputs.is_empty(), ports.outputs.is_empty()) {
            (true, _) => StageRole::Source,
            (_, true) => StageRole::Sink,
            _ => StageRole::Operation,
        }
    }
}

// ── StageRole ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRole {
    Source,
    Operation,
    Sink,
}
