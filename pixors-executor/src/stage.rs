use std::sync::Arc;
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

pub struct PortDecl {
    pub name: &'static str,
    pub kind: DataKind,
}

pub struct PortSpec {
    pub inputs: &'static [PortDecl],
    pub outputs: &'static [PortDecl],
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

// ── CpuKernel: a stage's CPU implementation ────────────────────────────────────
// NOTE: "CpuRunner" is reserved for the framework's per-thread runner entity (see
// runtime::runner). This trait is the per-stage CPU execution descriptor.

pub trait CpuKernel: Send {
    fn process(
        &mut self,
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

// ── GpuKernelDescriptor ────────────────────────────────────────────────────────

/// How the GPU kernel binds its primary input data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuInputBinding {
    /// Input is a single `Item::Tile`. One storage-read buffer.
    Tile,
    /// Input is an `Item::Neighborhood`. The runtime assembles the padded
    /// region into a single contiguous buffer before dispatching.
    Neighborhood,
}

/// Static description of a stage's GPU implementation.
/// Returned by value from `Stage::gpu_kernel_descriptor(&self)` so closures
/// can capture stage configuration (e.g. radius).
pub struct GpuKernelDescriptor {
    pub spirv: &'static [u8],
    pub entry_point: &'static str,
    pub input_binding: GpuInputBinding,
    /// (x, y) workgroup size declared in the shader. Used to compute dispatch dims.
    pub workgroup: (u32, u32),
    /// Byte size of the uniform params buffer (0 = no params).
    pub param_size: u64,
    /// Called by the runtime to fill the uniform buffer from the current item's
    /// metadata. `None` when `param_size == 0`.
    pub write_params: Option<Arc<dyn Fn(&crate::graph::item::Item, &mut [u8]) + Send + Sync>>,
}

// ── Stage trait ────────────────────────────────────────────────────────────────

pub trait Stage {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpec;
    fn hints(&self) -> StageHints;
    /// Tells the pipeline compiler which device this stage runs on and what
    /// input data it accepts. Stages override this to control Upload/Download
    /// insertion.
    fn device(&self) -> Device { Device::Cpu }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        None
    }
    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
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

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        match self {
            Self::Operation(n) => n.gpu_kernel_descriptor(),
            _ => None,
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
