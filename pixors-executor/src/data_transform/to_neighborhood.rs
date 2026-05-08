use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::data::neighborhood::{EdgeCondition, Neighborhood, TileGpuInfo};
use crate::data::tile::{Tile, TileCoord, TileGridPos};
use crate::gpu::pool::GpuBuffer;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor, ProcessorContext, Stage,
};
use serde::{Deserialize, Serialize};

use crate::error::Error;

use crate::debug_stopwatch;

static NA_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static NA_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "neighborhood",
    kind: DataKind::Neighborhood,
}];

static NA_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(NA_INPUTS),
    outputs: PortGroup::Fixed(NA_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileToNeighborhood {
    pub radius: u32,
}

impl Stage for TileToNeighborhood {
    fn kind(&self) -> &'static str {
        "neighborhood_agg"
    }

    fn ports(&self) -> &'static PortSpecification {
        &NA_PORTS
    }

    fn hints(&self) -> crate::stage::StageHints {
        crate::stage::StageHints::prefer_cpu()
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(TileToNeighborhoodProcessor::new(self.radius)))
    }
}

pub struct TileToNeighborhoodProcessor {
    pixel_radius: u32,
    emitted: HashSet<TileGridPos>,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    meta: Option<crate::common::pixel::meta::PixelMeta>,
    initialized: bool,
    is_gpu: Option<bool>,
    // CPU path: pointer accumulation
    tile_cache: HashMap<TileGridPos, Tile>,
    // GPU path: buffer accumulation + copy-to-consolidated
    gpu_buffers: HashMap<TileGridPos, Arc<GpuBuffer>>,
    gpu_tile_w: u32,
    gpu_tile_h: u32,
    gpu_ctx: Option<Arc<crate::gpu::context::GpuContext>>,
}

impl TileToNeighborhoodProcessor {
    pub fn new(pixel_radius: u32) -> Self {
        Self {
            pixel_radius,
            emitted: HashSet::new(),
            tile_size: 0,
            image_width: 0,
            image_height: 0,
            meta: None,
            initialized: false,
            is_gpu: None,
            tile_cache: HashMap::new(),
            gpu_buffers: HashMap::new(),
            gpu_tile_w: 0,
            gpu_tile_h: 0,
            gpu_ctx: None,
        }
    }

    fn tile_radius(&self) -> u32 {
        if self.tile_size == 0 {
            return 0;
        }
        self.pixel_radius.div_ceil(self.tile_size)
    }

    fn discover_bounds(&mut self, tile: &Tile) {
        if self.initialized {
            return;
        }
        self.meta = Some(tile.meta);
        self.tile_size = tile.coord.tile_size;
        self.image_width = tile.coord.image_width;
        self.image_height = tile.coord.image_height;
        self.initialized = true;
    }

    fn try_emit(&mut self, mip: u32, tx: u32, ty: u32, emit: &mut Emitter<Item>) {
        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };
        if self.emitted.contains(&key) {
            return;
        }
        let r = self.tile_radius() as i32;
        // At mip level m, each tile covers tile_size * 2^m original pixels.
        // tiles_x/y must count tiles in the mip-level grid, not the mip=0 grid.
        let mip_step = self.tile_size.saturating_mul(1u32 << mip);
        let tiles_x = self.image_width.div_ceil(mip_step) as i32;
        let tiles_y = self.image_height.div_ceil(mip_step) as i32;

        if self.is_gpu == Some(true) {
            self.try_emit_gpu(mip, tx, ty, r, tiles_x, tiles_y, emit);
        } else {
            self.try_emit_cpu(mip, tx, ty, r, tiles_x, tiles_y, emit);
        }
    }

    fn try_emit_cpu(
        &mut self,
        mip: u32,
        tx: u32,
        ty: u32,
        r: i32,
        tiles_x: i32,
        tiles_y: i32,
        emit: &mut Emitter<Item>,
    ) {
        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };

        let mut nbhd_tiles = Vec::new();
        let mut center_coord = None;
        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (tx as i32 + dx).clamp(0, tiles_x - 1).max(0) as u32;
                let gy = (ty as i32 + dy).clamp(0, tiles_y - 1).max(0) as u32;
                let nkey = TileGridPos {
                    mip_level: mip,
                    tx: gx,
                    ty: gy,
                };
                match self.tile_cache.get(&nkey) {
                    Some(tile) => {
                        if dx == 0 && dy == 0 {
                            center_coord = Some(tile.coord);
                        }
                        nbhd_tiles.push(tile.clone());
                    }
                    None => return,
                }
            }
        }

        let center = center_coord.unwrap_or_else(|| {
            nbhd_tiles
                .iter()
                .find(|t| t.coord.tx == tx && t.coord.ty == ty)
                .map(|t| t.coord)
                .unwrap_or(nbhd_tiles[0].coord)
        });

        let nbhd = Neighborhood::new_cpu(
            self.pixel_radius,
            center,
            nbhd_tiles,
            EdgeCondition::Clamp,
            self.meta.unwrap(),
            self.image_width,
            self.image_height,
            self.tile_size,
        );
        tracing::debug!(
            "[to_nbhd] emit CPU neighborhood center=({},{}) {}×{} radius={} tiles={}",
            center.px,
            center.py,
            center.width,
            center.height,
            self.pixel_radius,
            nbhd.data.tiles_cpu().len(),
        );
        self.emitted.insert(key);
        emit.emit(Item::Neighborhood(nbhd));
    }

    fn try_emit_gpu(
        &mut self,
        mip: u32,
        tx: u32,
        ty: u32,
        r: i32,
        tiles_x: i32,
        tiles_y: i32,
        emit: &mut Emitter<Item>,
    ) {
        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };

        let scheduler = self.gpu_ctx.as_ref().unwrap().scheduler();

        // Collect neighborhood tile buffers and build metadata
        let mut tile_infos = Vec::new();
        let mut total_bytes = 0u64;
        let mut center_px = 0u32;
        let mut center_py = 0u32;
        let mut center_w = 0u32;
        let mut center_h = 0u32;

        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (tx as i32 + dx).clamp(0, tiles_x - 1).max(0) as u32;
                let gy = (ty as i32 + dy).clamp(0, tiles_y - 1).max(0) as u32;
                let nkey = TileGridPos {
                    mip_level: mip,
                    tx: gx,
                    ty: gy,
                };
                match self.gpu_buffers.get(&nkey) {
                    Some(gbuf) => {
                        let bpp = self.meta.unwrap().format.bytes_per_pixel() as u64;
                        let tile_px = gx * self.tile_size;
                        let tile_py = gy * self.tile_size;
                        let tw = if gx == tiles_x as u32 - 1 {
                            self.image_width - tile_px
                        } else {
                            self.tile_size
                        };
                        let th = if gy == tiles_y as u32 - 1 {
                            self.image_height - tile_py
                        } else {
                            self.tile_size
                        };
                        let row_size = tw as u64 * bpp;
                        let data_size = row_size * th as u64;

                        if dx == 0 && dy == 0 {
                            center_px = tile_px;
                            center_py = tile_py;
                            center_w = tw;
                            center_h = th;
                        }

                        tile_infos.push(TileGpuInfo {
                            px: tile_px,
                            py: tile_py,
                            width: tw,
                            height: th,
                            data_offset: total_bytes,
                            tile_size_bytes: data_size,
                        });
                        total_bytes += gbuf.requested_size;
                    }
                    None => return, // not ready
                }
            }
        }

        // Allocate consolidated buffer and copy all tiles into it
        let consolidated = Arc::new(scheduler.allocate_buffer(total_bytes));
        for info in &tile_infos {
            let nkey = TileGridPos {
                mip_level: mip,
                tx: info.px / self.tile_size,
                ty: info.py / self.tile_size,
            };
            let gbuf = &self.gpu_buffers[&nkey];
            scheduler.copy_slice(
                gbuf.buffer(),
                0,
                consolidated.buffer(),
                info.data_offset,
                gbuf.requested_size,
            );
        }

        let center = TileCoord {
            mip_level: mip,
            tx,
            ty,
            px: center_px,
            py: center_py,
            width: center_w,
            height: center_h,
            tile_size: self.tile_size,
            image_width: self.image_width,
            image_height: self.image_height,
        };

        let nbhd = Neighborhood::new_gpu(
            self.pixel_radius,
            center,
            consolidated,
            tile_infos,
            EdgeCondition::Clamp,
            self.meta.unwrap(),
            self.image_width,
            self.image_height,
            self.tile_size,
        );
        self.emitted.insert(key);
        emit.emit(Item::Neighborhood(nbhd));
    }
}

impl Processor for TileToNeighborhoodProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("neighborhood_agg");
        let tile = ProcessorContext::take_tile(item)?;
        self.discover_bounds(&tile);

        let cur_ty = tile.coord.ty;
        let pos = TileGridPos {
            mip_level: tile.coord.mip_level,
            tx: tile.coord.tx,
            ty: cur_ty,
        };

        if self.is_gpu.is_none() {
            self.is_gpu = Some(ctx.device == Device::Gpu);
            if ctx.device == Device::Gpu {
                self.gpu_tile_w = tile.coord.width;
                self.gpu_tile_h = tile.coord.height;
                self.gpu_ctx = ctx.gpu.clone();
            }
        }

        if ctx.device == Device::Gpu {
            match &tile.data {
                Buffer::Gpu(gbuf) => {
                    self.gpu_buffers.insert(pos, gbuf.clone());
                }
                Buffer::Cpu(_) => {
                    return Err(Error::internal(
                        "TileToNeighborhood GPU path received CPU tile",
                    ));
                }
            }
        } else {
            self.tile_cache.insert(pos, tile);
        }

        let r = self.tile_radius();
        if cur_ty < r {
            return Ok(());
        }
        let safe_until = cur_ty - r;
        let candidates: Vec<TileGridPos> = if ctx.device == Device::Gpu {
            self.gpu_buffers
                .keys()
                .copied()
                .filter(|p| p.ty <= safe_until)
                .collect()
        } else {
            self.tile_cache
                .keys()
                .copied()
                .filter(|p| p.ty <= safe_until)
                .collect()
        };
        for pos in candidates {
            self.try_emit(pos.mip_level, pos.tx, pos.ty, ctx.emit);
        }
        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let keys: Vec<TileGridPos> = if self.is_gpu == Some(true) {
            self.gpu_buffers.keys().copied().collect()
        } else {
            self.tile_cache.keys().copied().collect()
        };
        for pos in keys {
            if !self.emitted.contains(&pos) {
                self.try_emit(pos.mip_level, pos.tx, pos.ty, ctx.emit);
            }
        }
        Ok(())
    }
}
