use std::fmt;

use half::f16;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use pixors_engine::data::buffer::{Buffer, GpuBuffer};
use pixors_engine::data::tile::{Tile, TileCoord, TileGridPos};
use pixors_engine::data::tile_block::{TileBlock, TileBlockCoord};
use pixors_engine::debug_stopwatch;
use pixors_engine::error::Error;
use pixors_engine::graph::emitter::Emitter;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
    StageHints,
};

static IN: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static OUT: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Fixed(IN),
    outputs: PortGroup::Fixed(OUT),
};

/// Generates MIP levels from incoming level-0 tiles.
///
/// For each incoming Tile, the stage:
///  1. Emits the tile as-is (pass-through).
///  2. Collects it into a 2×2 grid per MIP level.
///  3. When a 2×2 block is complete, box-averages it into one half-size tile
///     at `mip_level + 1`, emits it, and recursively ingests it to generate
///     even higher levels.
///
/// Tiles stay on their original device (GPU or CPU) throughout the chain.
/// GPU-backed blocks are downsampled via compute shader; CPU-backed via
/// software box-average.
#[derive(Clone)]
pub struct MipDownsample {
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
    gpu: Option<Arc<pixors_engine::gpu::context::GpuContext>>,
    grid: HashMap<TileGridPos, Tile>,
}

impl fmt::Debug for MipDownsample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MipDownsample")
            .field("image_width", &self.image_width)
            .field("image_height", &self.image_height)
            .field("tile_size", &self.tile_size)
            .field("grid", &self.grid)
            .finish()
    }
}

impl MipDownsample {
    pub fn new(image_width: u32, image_height: u32, tile_size: u32) -> Self {
        Self {
            image_width,
            image_height,
            tile_size,
            gpu: None,
            grid: HashMap::new(),
        }
    }
}

impl Processor for MipDownsample {
    fn kind(&self) -> &'static str {
        "mip_downsample"
    }
    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints::prefer_gpu()
    }
    fn work_multiplier(&self) -> f64 {
        1.33
    }

    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("mip_downsample");
        if self.gpu.is_none() {
            self.gpu = ctx.gpu.clone();
        }
        match item {
            Item::Tile(tile) => {
                ctx.emit.emit(Item::Tile(tile.clone()));
                self.ingest(tile, ctx.emit);
                Ok(())
            }
            Item::TileBlock(block) => {
                self.downsample_block(block, ctx.emit);
                Ok(())
            }
            _ => Err(Error::internal("MipDownsample expected Tile or TileBlock")),
        }
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        self.flush_remaining(ctx.emit);
        Ok(())
    }
}

impl MipDownsample {
    fn ingest(&mut self, tile: Tile, emit: &mut Emitter<Item>) {
        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;

        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };
        self.grid.insert(key, tile);

        // The 2×2 block's top-left corner (even-aligned).
        let tx_tl = (tx / 2) * 2;
        let ty_tl = (ty / 2) * 2;

        if self.is_block_ready(mip, tx_tl, ty_tl) {
            let tiles = self.take_block(mip, tx_tl, ty_tl);
            let coord = TileBlockCoord {
                mip_level: mip,
                tx_tl,
                ty_tl,
            };
            let block = TileBlock { coord, tiles };
            self.downsample_block(block, emit);
        }
    }

    fn is_block_ready(&self, mip: u32, tx_tl: u32, ty_tl: u32) -> bool {
        [
            TileGridPos {
                mip_level: mip,
                tx: tx_tl,
                ty: ty_tl,
            },
            TileGridPos {
                mip_level: mip,
                tx: tx_tl + 1,
                ty: ty_tl,
            },
            TileGridPos {
                mip_level: mip,
                tx: tx_tl,
                ty: ty_tl + 1,
            },
            TileGridPos {
                mip_level: mip,
                tx: tx_tl + 1,
                ty: ty_tl + 1,
            },
        ]
        .iter()
        .all(|k| self.grid.contains_key(k))
    }

    fn take_block(&mut self, mip: u32, tx_tl: u32, ty_tl: u32) -> [Tile; 4] {
        [
            self.grid
                .remove(&TileGridPos {
                    mip_level: mip,
                    tx: tx_tl,
                    ty: ty_tl,
                })
                .unwrap(),
            self.grid
                .remove(&TileGridPos {
                    mip_level: mip,
                    tx: tx_tl + 1,
                    ty: ty_tl,
                })
                .unwrap(),
            self.grid
                .remove(&TileGridPos {
                    mip_level: mip,
                    tx: tx_tl,
                    ty: ty_tl + 1,
                })
                .unwrap(),
            self.grid
                .remove(&TileGridPos {
                    mip_level: mip,
                    tx: tx_tl + 1,
                    ty: ty_tl + 1,
                })
                .unwrap(),
        ]
    }

    /// Box-average a 2×2 block into one tile at `mip + 1`, emit it, and
    /// recursively ingest it so even higher levels are generated.
    fn downsample_block(&mut self, block: TileBlock, emit: &mut Emitter<Item>) {
        let mip = block.coord.mip_level + 1;
        let tx = block.coord.tx_tl / 2;
        let ty = block.coord.ty_tl / 2;

        let all_gpu = block.tiles.iter().all(|t| t.data.is_gpu());
        let gpu_ctx = self.gpu.as_deref();
        let out_tile = if all_gpu {
            match gpu_ctx {
                Some(g) => {
                    match gpu_downsample_block(
                        g,
                        &block,
                        mip,
                        tx,
                        ty,
                        self.tile_size,
                        self.image_width,
                        self.image_height,
                    ) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!("GPU mip downsample failed ({e}), falling back to CPU");
                            cpu_downsample_block(
                                &block,
                                mip,
                                tx,
                                ty,
                                self.tile_size,
                                self.image_width,
                                self.image_height,
                            )
                        }
                    }
                }
                None => cpu_downsample_block(
                    &block,
                    mip,
                    tx,
                    ty,
                    self.tile_size,
                    self.image_width,
                    self.image_height,
                ),
            }
        } else {
            cpu_downsample_block(
                &block,
                mip,
                tx,
                ty,
                self.tile_size,
                self.image_width,
                self.image_height,
            )
        };

        emit.emit(Item::Tile(out_tile.clone()));

        let tiles_x = (self.image_width >> mip).max(1).div_ceil(self.tile_size);
        let tiles_y = (self.image_height >> mip).max(1).div_ceil(self.tile_size);
        if tiles_x > 1 || tiles_y > 1 {
            self.ingest(out_tile, emit);
        }
    }

    /// Flush all remaining tiles as partial blocks (edge/odd-sized images).
    fn flush_remaining(&mut self, emit: &mut Emitter<Item>) {
        let start_len = self.grid.len();
        loop {
            let min_mip = self.grid.keys().map(|k| k.mip_level).min();
            let Some(mip) = min_mip else { break };

            let block_tls: Vec<(u32, u32)> = self
                .grid
                .keys()
                .filter(|k| k.mip_level == mip)
                .map(|k| ((k.tx / 2) * 2, (k.ty / 2) * 2))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if block_tls.is_empty() {
                break;
            }

            for (tx_tl, ty_tl) in block_tls {
                let slots = [
                    TileGridPos {
                        mip_level: mip,
                        tx: tx_tl,
                        ty: ty_tl,
                    },
                    TileGridPos {
                        mip_level: mip,
                        tx: tx_tl + 1,
                        ty: ty_tl,
                    },
                    TileGridPos {
                        mip_level: mip,
                        tx: tx_tl,
                        ty: ty_tl + 1,
                    },
                    TileGridPos {
                        mip_level: mip,
                        tx: tx_tl + 1,
                        ty: ty_tl + 1,
                    },
                ];

                let filler_key = slots.iter().find(|k| self.grid.contains_key(k));
                let Some(filler_key) = filler_key else {
                    continue;
                };
                let filler = self.grid.get(filler_key).unwrap().clone();

                let tiles: [Tile; 4] = [
                    self.grid
                        .remove(&slots[0])
                        .unwrap_or_else(|| filler.clone()),
                    self.grid
                        .remove(&slots[1])
                        .unwrap_or_else(|| filler.clone()),
                    self.grid
                        .remove(&slots[2])
                        .unwrap_or_else(|| filler.clone()),
                    self.grid
                        .remove(&slots[3])
                        .unwrap_or_else(|| filler.clone()),
                ];

                let coord = TileBlockCoord {
                    mip_level: mip,
                    tx_tl,
                    ty_tl,
                };
                let block = TileBlock { coord, tiles };
                self.downsample_block(block, emit);
            }
        }
        debug_assert!(
            self.grid.is_empty(),
            "flush_remaining must drain all pending blocks"
        );
        if start_len > 0 {
            tracing::debug!(
                "[pixors] mip_downsample: flush_remaining flushed {} tiles",
                start_len,
            );
        }
    }
}

// ── GPU downsample ──────────────────────────────────────────────────────────

/// Downsample a 2×2 block of GPU tiles via compute shader.
/// Takes 4 GPU buffers directly — no download to CPU.
fn gpu_downsample_block(
    gpu: &pixors_engine::gpu::context::GpuContext,
    block: &TileBlock,
    mip: u32,
    tx: u32,
    ty: u32,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
) -> Result<Tile, Error> {
    let scheduler = gpu.scheduler();

    let gbufs: [&Arc<GpuBuffer>; 4] = [
        block.tiles[0].data.as_gpu().unwrap(),
        block.tiles[1].data.as_gpu().unwrap(),
        block.tiles[2].data.as_gpu().unwrap(),
        block.tiles[3].data.as_gpu().unwrap(),
    ];
    let coord = TileCoord::new(mip, tx, ty, tile_size, image_width, image_height);
    let fmt = block.tiles[0].meta.format;
    let bpp = fmt.bytes_per_pixel();
    let out_w = coord.width;
    let out_h = coord.height;
    let out_size = (out_w * out_h * bpp as u32) as u64;

    let kernel = pixors_shader::kernel::mip_downsample::MipParamsKernel::new(
        pixors_shader::kernel::mip_downsample::MipParams {
            out_width: out_w,
            out_height: out_h,
            w0: block.tiles[0].coord.width,
            h0: block.tiles[0].coord.height,
            w1: block.tiles[1].coord.width,
            h1: block.tiles[1].coord.height,
            w2: block.tiles[2].coord.width,
            h2: block.tiles[2].coord.height,
            w3: block.tiles[3].coord.width,
            h3: block.tiles[3].coord.height,
        },
        fmt,
    );
    let dispatch_x = out_w.div_ceil(8);
    let dispatch_y = out_h.div_ceil(8);

    let out_gbuf = scheduler.allocate_buffer(out_size);

    let out_gbuf = scheduler
        .dispatch_one(&kernel, &gbufs, out_gbuf, dispatch_x, dispatch_y)
        .map_err(|e| Error::internal(format!("mip downsample GPU: {e}")))?;

    let meta = block.tiles[0].meta;
    Ok(Tile::new(coord, meta, Buffer::Gpu(Arc::new(out_gbuf))))
}

// ── CPU downsample ──────────────────────────────────────────────────────────

/// Downsample a 2×2 block of CPU tiles by box-averaging.
/// Supports RGBA8 (byte-level averaging) and RGBAF16 (proper half-float averaging).
fn cpu_downsample_block(
    block: &TileBlock,
    mip: u32,
    tx: u32,
    ty: u32,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
) -> Tile {
    let meta = block.tiles[0].meta;
    let coord = TileCoord::new(mip, tx, ty, tile_size, image_width, image_height);
    let out_w = coord.width as usize;
    let out_h = coord.height as usize;

    if out_w == 0 || out_h == 0 {
        return Tile::new(coord, meta, Buffer::cpu(vec![]));
    }

    let bpp = meta.format.bytes_per_pixel();
    let mut out = vec![0u8; out_w * out_h * bpp];

    let w0 = block.tiles[0].coord.width as usize;
    let h0 = block.tiles[0].coord.height as usize;
    let tiles = &block.tiles;

    use pixors_engine::common::pixel::PixelFormat;

    match meta.format {
        PixelFormat::RgbaF16 | PixelFormat::RgbF16 => {
            out.par_chunks_mut(out_w * bpp)
                .enumerate()
                .for_each(|(y, row)| {
                    for x in 0..out_w {
                        let avg = sample_average_f16(tiles, x * 2, y * 2, w0, h0);
                        let off = x * bpp;
                        row[off..off + bpp].copy_from_slice(&avg);
                    }
                });
        }
        PixelFormat::RgbaF32 | PixelFormat::RgbF32 => {
            out.par_chunks_mut(out_w * bpp)
                .enumerate()
                .for_each(|(y, row)| {
                    for x in 0..out_w {
                        let avg = sample_average_f32(tiles, x * 2, y * 2, w0, h0);
                        let off = x * bpp;
                        row[off..off + bpp].copy_from_slice(&avg);
                    }
                });
        }
        _ => {
            out.par_chunks_mut(out_w * bpp)
                .enumerate()
                .for_each(|(y, row)| {
                    for x in 0..out_w {
                        let avg = sample_and_average(tiles, x * 2, y * 2, w0, h0, bpp);
                        let off = x * bpp;
                        row[off..off + bpp].copy_from_slice(&avg);
                    }
                });
        }
    }

    Tile::new(coord, meta, Buffer::cpu(out))
}

/// Byte-level box average for any format (safe for RGBA8).
fn sample_and_average(
    tiles: &[Tile; 4],
    sx: usize,
    sy: usize,
    w0: usize,
    h0: usize,
    bpp: usize,
) -> Vec<u8> {
    let mut sum = vec![0u32; bpp];
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let px = sx + dx;
        let py = sy + dy;

        let (tile_idx, lx, ly) = if py < h0 {
            if px < w0 {
                (0, px, py)
            } else {
                (1, px - w0, py)
            }
        } else {
            if px < w0 {
                (2, px, py - h0)
            } else {
                (3, px - w0, py - h0)
            }
        };

        let tile = &tiles[tile_idx];
        let tw = tile.coord.width as usize;
        let th = tile.coord.height as usize;
        let cx = lx.min(tw.saturating_sub(1));
        let cy = ly.min(th.saturating_sub(1));

        let data: &[u8] = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => &[],
        };
        let off = (cy * tw + cx) * bpp;
        if off + bpp <= data.len() {
            for c in 0..bpp {
                sum[c] += data[off + c] as u32;
            }
        }
    }

    sum.iter().map(|s| (s / 4) as u8).collect()
}

/// Box average for f32 formats — each channel is 4 bytes IEEE 754.
fn sample_average_f32(tiles: &[Tile; 4], sx: usize, sy: usize, w0: usize, h0: usize) -> Vec<u8> {
    let bpp = 16usize; // 4 channels × 4 bytes
    let mut sum = [0.0f32; 4];
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let px = sx + dx;
        let py = sy + dy;

        let (tile_idx, lx, ly) = if py < h0 {
            if px < w0 {
                (0, px, py)
            } else {
                (1, px - w0, py)
            }
        } else {
            if px < w0 {
                (2, px, py - h0)
            } else {
                (3, px - w0, py - h0)
            }
        };

        let tile = &tiles[tile_idx];
        let tw = tile.coord.width as usize;
        let th = tile.coord.height as usize;
        let cx = lx.min(tw.saturating_sub(1));
        let cy = ly.min(th.saturating_sub(1));

        let data: &[u8] = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => &[],
        };
        let off = (cy * tw + cx) * bpp;
        if off + bpp <= data.len() {
            for c in 0..4 {
                let bytes = [
                    data[off + c * 4],
                    data[off + c * 4 + 1],
                    data[off + c * 4 + 2],
                    data[off + c * 4 + 3],
                ];
                sum[c] += f32::from_le_bytes(bytes);
            }
        }
    }

    let mut out = Vec::with_capacity(bpp);
    for s in &sum {
        out.extend_from_slice(&(s / 4.0).to_le_bytes());
    }
    out
}

/// Box average for f16 formats — each channel pair is a half-float.
fn sample_average_f16(tiles: &[Tile; 4], sx: usize, sy: usize, w0: usize, h0: usize) -> Vec<u8> {
    let bpp = 8usize; // 4 channels × 2 bytes
    let mut sum = [0.0f32; 4];
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let px = sx + dx;
        let py = sy + dy;

        let (tile_idx, lx, ly) = if py < h0 {
            if px < w0 {
                (0, px, py)
            } else {
                (1, px - w0, py)
            }
        } else {
            if px < w0 {
                (2, px, py - h0)
            } else {
                (3, px - w0, py - h0)
            }
        };

        let tile = &tiles[tile_idx];
        let tw = tile.coord.width as usize;
        let th = tile.coord.height as usize;
        let cx = lx.min(tw.saturating_sub(1));
        let cy = ly.min(th.saturating_sub(1));

        let data: &[u8] = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => &[],
        };
        let off = (cy * tw + cx) * bpp;
        if off + bpp <= data.len() {
            for c in 0..4 {
                let lo = data[off + c * 2] as u16;
                let hi = data[off + c * 2 + 1] as u16;
                let bits = lo | (hi << 8);
                sum[c] += f32::from(f16::from_bits(bits));
            }
        }
    }

    let mut out = Vec::with_capacity(bpp);
    for s in &sum {
        let bits = f16::from_f32(s / 4.0).to_bits();
        out.push(bits as u8);
        out.push((bits >> 8) as u8);
    }
    out
}
