use std::sync::Arc;

use crate::cache::disk_cache::DiskCache;
use crate::data::buffer::Buffer;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};

static CW_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CW_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(CW_INPUTS),
};

/// Writes tiles to a DiskCache (disk + in-memory LRU).
#[derive(Debug, Clone)]
pub struct CacheWriter {
    pub cache: Arc<DiskCache>,
}

impl CacheWriter {
    pub fn new(cache: Arc<DiskCache>) -> Self {
        Self { cache }
    }
}

impl Consumer for CacheWriter {
    fn kind(&self) -> &'static str {
        "cache_writer"
    }

    fn in_ports(&self) -> &'static InPortSpecification {
        &CW_IN_PORTS
    }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = crate::stage::ProcessorContext::take_tile(item)?;
        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;
        let w = tile.coord.width;
        let h = tile.coord.height;
        if w == 0 || h == 0 {
            return Ok(());
        }

        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("CacheWriter requires CPU tiles")),
        };
        let bpp = tile.meta.format.bytes_per_pixel();
        let expected = w as usize * h as usize * bpp;
        if data.len() != expected {
            return Err(Error::internal(format!(
                "CacheWriter tile size mismatch: expected {expected} bytes, got {}",
                data.len(),
            )));
        }

        self.cache
            .write_tile(mip, tx, ty, data)
            .map_err(|e| Error::internal(format!("CacheWriter: {e}")))?;
        Ok(())
    }
}
