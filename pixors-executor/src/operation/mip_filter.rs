use serde::{Deserialize, Serialize};

use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortGroup, PortSpec, Stage, StageHints};

use crate::error::Error;


static IN: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];

static OUT: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];

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
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(MipFilterRunner { mip_level: self.mip_level }))
    }
}

struct MipFilterRunner {
    mip_level: u32,
}

impl CpuKernel for MipFilterRunner {
    fn process(&mut self, _port: u16, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        if let Item::Tile(tile) = item {
            if tile.coord.mip_level == self.mip_level {
                emit.emit(Item::Tile(tile));
            }
        }
        Ok(())
    }
}
