use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::common::color::space::ColorSpace;
use crate::common::pixel::meta::PixelMeta;
use crate::common::pixel::{AlphaPolicy, PixelFormat};
use crate::data::buffer::Buffer;
use crate::data::tile::{Tile, TileCoord};
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, ProcessorContext, Producer, Stage,
};

static CR_INPUTS: &[PortDeclaration] = &[];
static CR_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CR_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(CR_INPUTS),
    outputs: PortGroup::Fixed(CR_OUTPUTS),
};

/// Bounding range of tile coordinates (exclusive end).
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TileRange {
    pub tx_start: u32,
    pub tx_end: u32,
    pub ty_start: u32,
    pub ty_end: u32,
}

impl Clone for TileRange {
    fn clone(&self) -> Self {
        Self {
            tx_start: self.tx_start,
            tx_end: self.tx_end,
            ty_start: self.ty_start,
            ty_end: self.ty_end,
        }
    }
}

/// Reads tiles from a disk cache written by [`CacheWriter`](crate::sink::cache_writer::CacheWriter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheReader {
    pub cache_dir: PathBuf,
    pub mip_level: u32,
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub tile_range: Option<TileRange>,
}

impl Stage for CacheReader {
    fn kind(&self) -> &'static str {
        "cache_reader"
    }

    fn ports(&self) -> &'static PortSpecification {
        &CR_PORTS
    }

    fn producer(&self) -> Option<Box<dyn Producer>> {
        Some(Box::new(CacheReaderProducer {
            cache_dir: self.cache_dir.clone(),
            mip_level: self.mip_level,
            tile_size: self.tile_size,
            image_width: self.image_width,
            image_height: self.image_height,
            tile_range: self.tile_range.clone(),
            use_compression: false,
        }))
    }

    fn source_items(&self) -> usize {
        let mip_w = (self.image_width >> self.mip_level).max(1);
        let mip_h = (self.image_height >> self.mip_level).max(1);
        let cols = mip_w.div_ceil(self.tile_size);
        let rows = mip_h.div_ceil(self.tile_size);

        let (tx_start, tx_end, ty_start, ty_end) = match &self.tile_range {
            Some(r) => (
                r.tx_start,
                r.tx_end.min(cols),
                r.ty_start,
                r.ty_end.min(rows),
            ),
            None => (0, cols, 0, rows),
        };

        (tx_end.saturating_sub(tx_start) * ty_end.saturating_sub(ty_start)) as usize
    }
}

pub struct CacheReaderProducer {
    cache_dir: PathBuf,
    mip_level: u32,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    tile_range: Option<TileRange>,
    use_compression: bool,
}

impl Producer for CacheReaderProducer {
    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let mip_w = (self.image_width >> self.mip_level).max(1);
        let mip_h = (self.image_height >> self.mip_level).max(1);
        let cols = mip_w.div_ceil(self.tile_size);
        let rows = mip_h.div_ceil(self.tile_size);

        let (tx_start, tx_end, ty_start, ty_end) = match &self.tile_range {
            Some(r) => (
                r.tx_start,
                r.tx_end.min(cols),
                r.ty_start,
                r.ty_end.min(rows),
            ),
            None => (0, cols, 0, rows),
        };

        let meta = PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight);
        let dir = self.cache_dir.join(format!("mip_{}", self.mip_level));

        if !dir.is_dir() {
            tracing::warn!(
                "[pixors] cache_reader: cache dir does not exist: {}",
                dir.display()
            );
            return Ok(());
        }

        for ty in ty_start..ty_end {
            for tx in tx_start..tx_end {
                let path = dir.join(format!("tile_{}_{}_{}.raw", self.mip_level, tx, ty));
                let bytes = match std::fs::read(&path) {
                    Ok(b) => b,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        tracing::warn!(
                            "[pixors] cache_reader: failed to read {}: {e}",
                            path.display()
                        );
                        continue;
                    }
                };

                let bytes = if self.use_compression {
                    match lz4_flex::decompress_size_prepended(&bytes) {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!(
                                "[pixors] cache_reader: failed to decompress {}: {e}",
                                path.display()
                            );
                            continue;
                        }
                    }
                } else {
                    bytes
                };

                let coord = TileCoord::new(self.mip_level, tx, ty, self.tile_size, mip_w, mip_h);
                if coord.width == 0 || coord.height == 0 {
                    continue;
                }
                ctx.emit
                    .emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(bytes))));
            }
        }
        Ok(())
    }
}
