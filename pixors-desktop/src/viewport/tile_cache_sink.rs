use std::fmt;
use std::sync::Arc;

use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};

pub type CacheCommitFn =
    Arc<dyn Fn(u64, u64, u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;

static VCS_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static VCS_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(VCS_INPUTS),
};

pub struct TileCacheSink {
    pub generation: u64,
    pub version: u64,
    callback: CacheCommitFn,
}

impl fmt::Debug for TileCacheSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileCacheSink")
            .field("generation", &self.generation)
            .field("version", &self.version)
            .finish()
    }
}

impl TileCacheSink {
    pub fn new(generation: u64, version: u64, callback: CacheCommitFn) -> Self {
        Self {
            generation,
            version,
            callback,
        }
    }
}

impl Consumer for TileCacheSink {
    fn kind(&self) -> &'static str {
        "tile_cache_sink"
    }
    fn in_ports(&self) -> &'static InPortSpecification {
        &VCS_IN_PORTS
    }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("TileCacheSink requires CPU tiles")),
        };
        (self.callback)(
            self.generation,
            self.version,
            tile.coord.mip_level,
            tile.coord.tx,
            tile.coord.ty,
            tile.coord.px,
            tile.coord.py,
            tile.coord.width,
            tile.coord.height,
            data,
        );
        Ok(())
    }
}
