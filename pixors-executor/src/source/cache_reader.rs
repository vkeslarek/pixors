use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::{Buffer, Tile, TileCoord};
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::model::color::ColorSpace;
use crate::model::pixel::meta::PixelMeta;
use crate::model::pixel::{AlphaPolicy, PixelFormat};
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortGroup, PortSpec, Stage, StageHints};

static CR_INPUTS: &[PortDecl] = &[];
static CR_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static CR_PORTS: PortSpec = PortSpec { inputs: PortGroup::Fixed(CR_INPUTS), outputs: PortGroup::Fixed(CR_OUTPUTS) };

/// Bounding range of tile coordinates (exclusive end).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileRange {
    pub tx_start: u32,
    pub tx_end: u32,
    pub ty_start: u32,
    pub ty_end: u32,
}

/// Reads tiles from a disk cache written by [`CacheWriter`](crate::sink::cache_writer::CacheWriter).
///
/// A source stage (0 inputs) — reads all tiles in the given MIP level and
/// emits them on the first (dummy) invocation.
///
/// Missing files are silently skipped (edge / partial tiles may not exist).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheReader {
    pub cache_dir: PathBuf,
    pub mip_level: u32,
    pub tile_size: u32,
    /// Dimensions at MIP 0 (used to compute the grid at the target MIP).
    pub image_width: u32,
    pub image_height: u32,
    /// Only emit tiles within this range.  `None` emits every tile.
    pub tile_range: Option<TileRange>,
}

impl Stage for CacheReader {
    fn kind(&self) -> &'static str {
        "cache_reader"
    }

    fn ports(&self) -> &'static PortSpec {
        &CR_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(CacheReaderRunner {
            cache_dir: self.cache_dir.clone(),
            mip_level: self.mip_level,
            tile_size: self.tile_size,
            image_width: self.image_width,
            image_height: self.image_height,
            tile_range: self.tile_range.clone(),
        }))
    }
}

pub struct CacheReaderRunner {
    cache_dir: PathBuf,
    mip_level: u32,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    tile_range: Option<TileRange>,
}

impl CpuKernel for CacheReaderRunner {
    fn process(&mut self, _port: u16, _item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
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

        let meta = PixelMeta::new(
            PixelFormat::Rgba8,
            ColorSpace::SRGB,
            AlphaPolicy::Straight,
        );
        let dir = self.cache_dir.join(format!("mip_{}", self.mip_level));

        if !dir.is_dir() {
            tracing::warn!(
                "[pixors] cache_reader: cache dir does not exist: {}",
                dir.display(),
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
                            path.display(),
                        );
                        continue;
                    }
                };

                let coord = TileCoord::new(
                    self.mip_level,
                    tx,
                    ty,
                    self.tile_size,
                    mip_w,
                    mip_h,
                );
                if coord.width == 0 || coord.height == 0 {
                    continue;
                }
                emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(bytes))));
            }
        }
        Ok(())
    }
}
