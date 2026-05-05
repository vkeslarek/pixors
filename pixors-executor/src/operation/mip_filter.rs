use serde::{Deserialize, Serialize};

use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};

use crate::error::Error;

static IN: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static OUT: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(IN),
    outputs: PortGroup::Fixed(OUT),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MipFilter {
    pub mip_level: u32,
}

impl Stage for MipFilter {
    fn kind(&self) -> &'static str {
        "mip_filter"
    }
    fn ports(&self) -> &'static PortSpecification {
        &PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(MipFilterProcessor {
            mip_level: self.mip_level,
        }))
    }
}

struct MipFilterProcessor {
    mip_level: u32,
}

impl Processor for MipFilterProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let tile = ProcessorContext::take_tile(item)?;
        if tile.coord.mip_level == self.mip_level {
            ctx.emit.emit(Item::Tile(tile));
        }
        Ok(())
    }
}
