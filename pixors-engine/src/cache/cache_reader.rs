use std::sync::Arc;

use crate::cache::disk_cache::DiskCache;
use crate::common::color::space::ColorSpace;
use crate::common::pixel::meta::PixelMeta;
use crate::common::pixel::{AlphaPolicy, PixelFormat};
use crate::data::buffer::Buffer;
use crate::data::tile::{Tile, TileCoord};
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, OutPortSpecification, PortDeclaration, PortGroup, ProcessorContext, Producer,
};

static CR_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CR_OUT_PORTS: OutPortSpecification = OutPortSpecification {
    ports: PortGroup::Fixed(CR_OUTPUTS),
};

/// Bounding range of tile coordinates (exclusive end).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileRange {
    pub tx_start: u32,
    pub tx_end: u32,
    pub ty_start: u32,
    pub ty_end: u32,
}

/// Reads tiles from a DiskCache (LRU first, disk fallback).
#[derive(Debug, Clone)]
pub struct CacheReader {
    pub cache: Arc<DiskCache>,
    pub mip_level: u32,
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub tile_range: Option<TileRange>,
    pub pixel_format: PixelFormat,
    pub color_space: ColorSpace,
}

impl Producer for CacheReader {
    fn kind(&self) -> &'static str {
        "cache_reader"
    }

    fn out_ports(&self) -> &'static OutPortSpecification {
        &CR_OUT_PORTS
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

        let meta = PixelMeta::new(self.pixel_format, self.color_space, AlphaPolicy::Straight);

        for ty in ty_start..ty_end {
            for tx in tx_start..tx_end {
                let bytes = match self.cache.read_tile(self.mip_level, tx, ty) {
                    Some(b) => b,
                    None => continue,
                };
                let coord = TileCoord::new(
                    self.mip_level,
                    tx,
                    ty,
                    self.tile_size,
                    self.image_width,
                    self.image_height,
                );
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
