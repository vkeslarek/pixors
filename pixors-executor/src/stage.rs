use serde::{Deserialize, Serialize};
use crate::data::device::Device;
use crate::data_transform::DataTransformNode;
use crate::error::Error;
use crate::operation::OperationNode;
use crate::sink::SinkNode;
use crate::source::SourceNode;

// ── Data kinds ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Tile,
    TileBlock,
    Neighborhood,
    ScanLine,
}

#[derive(Debug, Clone, Copy)]
pub struct PortDeclaration {
    pub name: &'static str,
    pub kind: DataKind,
}

#[derive(Debug, Clone, Copy)]
pub enum PortGroup {
    Fixed(&'static [PortDeclaration]),
    Variable(&'static PortDeclaration),
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

pub struct PortSpecification {
    pub inputs: PortGroup,
    pub outputs: PortGroup,
}

// ── Buffer access hint ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferAccess {
    ReadOnly,
    ReadWriteInPlace,
    ReadTransform,
}

// ── Stage hints ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StageHints {
    pub buffer_access: BufferAccess,
    pub prefers_gpu: bool,
}

// ── Processor ─────────────────────────────────────────────────────────────────

pub struct ProcessorContext<'a> {
    pub port: u16,
    pub device: Device,
    pub emit: &'a mut crate::graph::emitter::Emitter<crate::graph::item::Item>,
}

impl<'a> ProcessorContext<'a> {
    pub fn ensure_cpu(&self) -> Result<(), Error> {
        if self.device == Device::Cpu {
            Ok(())
        } else {
            Err(Error::internal(format!("expected CPU, got {:?}", self.device)))
        }
    }

    pub fn ensure_gpu(&self) -> Result<(), Error> {
        if self.device == Device::Gpu {
            Ok(())
        } else {
            Err(Error::internal(format!("expected GPU, got {:?}", self.device)))
        }
    }

    pub fn take_tile(item: crate::graph::item::Item) -> Result<crate::data::tile::Tile, Error> {
        match item {
            crate::graph::item::Item::Tile(t) => Ok(t),
            other => Err(Error::internal(format!("expected Tile, got {:?}", other.kind()))),
        }
    }

    pub fn take_scanline(item: crate::graph::item::Item) -> Result<crate::data::scanline::ScanLine, Error> {
        match item {
            crate::graph::item::Item::ScanLine(s) => Ok(s),
            other => Err(Error::internal(format!("expected ScanLine, got {:?}", other.kind()))),
        }
    }

    pub fn take_neighborhood(item: crate::graph::item::Item) -> Result<crate::data::neighborhood::Neighborhood, Error> {
        match item {
            crate::graph::item::Item::Neighborhood(n) => Ok(n),
            other => Err(Error::internal(format!("expected Neighborhood, got {:?}", other.kind()))),
        }
    }

    pub fn take_tile_block(item: crate::graph::item::Item) -> Result<crate::data::tile_block::TileBlock, Error> {
        match item {
            crate::graph::item::Item::TileBlock(b) => Ok(b),
            other => Err(Error::internal(format!("expected TileBlock, got {:?}", other.kind()))),
        }
    }
}

pub trait Processor: Send {
    fn process(
        &mut self,
        ctx: ProcessorContext<'_>,
        item: crate::graph::item::Item,
    ) -> Result<(), Error>;
    fn finish(
        &mut self,
        _ctx: ProcessorContext<'_>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

// ── Stage trait ────────────────────────────────────────────────────────────────

pub trait Stage {
    fn kind(&self) -> &'static str;
    fn ports(&self) -> &'static PortSpecification;
    fn hints(&self) -> StageHints;
    fn device(&self) -> Device { Device::Cpu }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        None
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
            fn device(&self) -> $crate::data::device::Device {
                match self { $(Self::$variant(n) => n.device()),+ }
            }
            fn processor(&self) -> Option<Box<dyn $crate::stage::Processor>> {
                match self { $(Self::$variant(n) => n.processor()),+ }
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
