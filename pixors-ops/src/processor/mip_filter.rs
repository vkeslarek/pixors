use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor, ProcessorContext, Stage,
};
use serde::{Deserialize, Serialize};

use pixors_engine::error::Error;

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
    fn hints(&self) -> pixors_engine::stage::StageHints {
        pixors_engine::stage::StageHints::either()
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
