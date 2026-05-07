use crate::data::device::Device;
use crate::data_transform::DataTransformNode;
use crate::operation::OperationNode;
use crate::sink::SinkNode;
use crate::source::SourceNode;
use serde::{Deserialize, Serialize};

use super::actors::{Consumer, Producer, Processor};
use super::kinds::PortSpecification;

// ── Stage trait ────────────────────────────────────────────────────────────────

pub trait Stage {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpecification;
    fn device(&self) -> Device {
        Device::Cpu
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
            fn device(&self) -> $crate::data::device::Device {
                match self { $(Self::$variant(n) => n.device()),+ }
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
