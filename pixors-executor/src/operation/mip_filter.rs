use serde::{Deserialize, Serialize};

use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, Processor, DataKind, PortDeclaration, PortGroup, PortSpec, Stage, StageHints};

use crate::error::Error;


static IN: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static OUT: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static PORTS: PortSpec = PortSpec { inputs: PortGroup::Fixed(IN), outputs: PortGroup::Fixed(OUT) };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MipFilter {
    pub mip_level: u32,
}

impl Stage for MipFilter {
    fn kind(&self) -> &'static str { "mip_filter" }
    fn ports(&self) -> &'static PortSpec { &PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadOnly, prefers_gpu: false }
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(MipFilterProcessor { mip_level: self.mip_level }))
    }
}

struct MipFilterProcessor {
    mip_level: u32,
}

impl Processor for MipFilterProcessor {
    fn process(&mut self, _port: u16, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        if let Item::Tile(tile) = item {
            if tile.coord.mip_level == self.mip_level {
                emit.emit(Item::Tile(tile));
            }
        }
        Ok(())
    }
}
