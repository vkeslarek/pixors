use serde::{Deserialize, Serialize};
use crate::data::Device;
use crate::data_transform::DataTransformNode;
use crate::error::Error;
use crate::source::SourceNode;
use crate::sink::SinkNode;
use crate::operation::OperationNode;

// ── Data kinds ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Tile,
    TileBlock,
    Neighborhood,
    ScanLine,
}

#[derive(Debug, Clone, Copy)]
pub struct PortDecl {
    pub name: &'static str,
    pub kind: DataKind,
}

#[derive(Debug, Clone, Copy)]
pub enum PortGroup {
    Fixed(&'static [PortDecl]),
    Variable(&'static PortDecl),
}

impl PortGroup {
    pub fn is_empty(&self) -> bool {
        match self {
            PortGroup::Fixed(ports) => ports.is_empty(),
            PortGroup::Variable(_) => false,
        }
    }

    pub fn kind_at(&self, index: usize) -> Option<DataKind> {
        match self {
            PortGroup::Fixed(ports) => ports.get(index).map(|p| p.kind),
            PortGroup::Variable(decl) => Some(decl.kind),
        }
    }

    pub fn name_at(&self, index: usize) -> Option<&'static str> {
        match self {
            PortGroup::Fixed(ports) => ports.get(index).map(|p| p.name),
            PortGroup::Variable(decl) => Some(decl.name),
        }
    }
}

pub struct PortSpec {
    pub inputs: PortGroup,
    pub outputs: PortGroup,
}

// ── Buffer access hint ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferAccess {
    /// Reads input buffer, never modifies it.
    ReadOnly,
    /// Reads and modifies the input buffer in place. Runtime inserts a copy when
    /// this node's input is shared with another downstream node.
    ReadWriteInPlace,
    /// Reads from input buffer and writes to a freshly allocated output buffer.
    ReadTransform,
}

// ── Stage hints ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StageHints {
    pub buffer_access: BufferAccess,
    /// When true and a GPU is available, the runtime schedules this stage on GPU.
    pub prefers_gpu: bool,
}

// ── Kernel: a stage's execution implementation ────────────────────────────────
// Per-stage kernel; may dispatch GPU work internally. The framework's per-thread
// runner entity is `ChainRunner` (see runtime::runner).

pub trait CpuKernel: Send {
    fn process(
        &mut self,
        port: u16,
        item: crate::graph::item::Item,
        emit: &mut crate::graph::emitter::Emitter<crate::graph::item::Item>,
    ) -> Result<(), Error>;
    fn finish(
        &mut self,
        _emit: &mut crate::graph::emitter::Emitter<crate::graph::item::Item>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

// ── Stage trait ────────────────────────────────────────────────────────────────

pub trait Stage {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpec;
    fn hints(&self) -> StageHints;
    /// Tells the pipeline compiler which device this stage runs on.
    /// Controls Upload/Download insertion at device-crossing edges.
    fn device(&self) -> Device { Device::Cpu }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        None
    }
}

// ── StageNode: serialisable wrapper ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StageNode {
    Source(SourceNode),
    Sink(SinkNode),
    Operation(OperationNode),
    DataTransform(DataTransformNode),
}

impl Stage for StageNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::Source(n) => n.kind(),
            Self::Sink(n) => n.kind(),
            Self::Operation(n) => n.kind(),
            Self::DataTransform(n) => n.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::Source(n) => n.ports(),
            Self::Sink(n) => n.ports(),
            Self::Operation(n) => n.ports(),
            Self::DataTransform(n) => n.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::Source(n) => n.hints(),
            Self::Sink(n) => n.hints(),
            Self::Operation(n) => n.hints(),
            Self::DataTransform(n) => n.hints(),
        }
    }

    fn device(&self) -> Device {
        match self {
            Self::Source(n) => n.device(),
            Self::Sink(n) => n.device(),
            Self::Operation(n) => n.device(),
            Self::DataTransform(n) => n.device(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::Source(n) => n.cpu_kernel(),
            Self::Sink(n) => n.cpu_kernel(),
            Self::Operation(n) => n.cpu_kernel(),
            Self::DataTransform(n) => n.cpu_kernel(),
        }
    }
}

impl StageNode {
    /// Derived from PortSpec: no inputs → Source, no outputs → Sink, else Operation.
    pub fn role(&self) -> StageRole {
        let ports = self.ports();
        match (ports.inputs.is_empty(), ports.outputs.is_empty()) {
            (true, _) => StageRole::Source,
            (_, true) => StageRole::Sink,
            _ => StageRole::Operation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRole {
    Source,
    Operation,
    Sink,
}
