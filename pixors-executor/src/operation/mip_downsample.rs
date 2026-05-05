use std::collections::HashMap;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::data::{Buffer, GpuBuffer, Tile, TileBlock, TileBlockCoord, TileCoord, TileGridPos};
use crate::debug_stopwatch;
use crate::error::Error;
use crate::gpu;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ParameterDeclaration, ParameterType, ResourceDeclaration,
};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, CpuKernel, DataKind, PortDecl, PortGroup, PortSpec, Stage, StageHints,
};

const MIP_DOWNSAMPLE_SPV: &[u8] =
    include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/mip_downsample.spv"));

static IN: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static OUT: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static PORTS: PortSpec = PortSpec { inputs: PortGroup::Fixed(IN), outputs: PortGroup::Fixed(OUT) };

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MipDownsample {
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
}

impl Stage for MipDownsample {
    fn kind(&self) -> &'static str { "mip_downsample" }
    fn ports(&self) -> &'static PortSpec { &PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadTransform, prefers_gpu: false }
    }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(MipDownsampleRunner::new(
            self.image_width, self.image_height, self.tile_size,
        )))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

pub struct MipDownsampleRunner {
    image_width: u32,
    image_height: u32,
    tile_size: u32,
    /// Tiles waiting for their 2×2 block partner, keyed by grid position.
    grid: HashMap<TileGridPos, Tile>,
}

impl MipDownsampleRunner {
    pub fn new(image_width: u32, image_height: u32, tile_size: u32) -> Self {
        Self { image_width, image_height, tile_size, grid: HashMap::new() }
    }

    /// Insert a tile into the grid and try to form a 2×2 block.
    /// If a block is complete, downsample it and recursively insert the
    /// resulting higher-level tile.
    fn ingest(&mut self, tile: Tile, emit: &mut Emitter<Item>) {
        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;

        let key = TileGridPos { mip_level: mip, tx, ty };
        self.grid.insert(key, tile);

        // The 2×2 block's top-left corner (even-aligned).
        let tx_tl = (tx / 2) * 2;
        let ty_tl = (ty / 2) * 2;

        if self.is_block_ready(mip, tx_tl, ty_tl) {
            let tiles = self.take_block(mip, tx_tl, ty_tl);
            let coord = TileBlockCoord { mip_level: mip, tx_tl, ty_tl };
            let block = TileBlock { coord, tiles };
            self.downsample_block(block, emit);
        }
    }

    fn is_block_ready(&self, mip: u32, tx_tl: u32, ty_tl: u32) -> bool {
        [
            TileGridPos { mip_level: mip, tx: tx_tl,     ty: ty_tl },
            TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl },
            TileGridPos { mip_level: mip, tx: tx_tl,     ty: ty_tl + 1 },
            TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl + 1 },
        ]
        .iter()
        .all(|k| self.grid.contains_key(k))
    }

    fn take_block(&mut self, mip: u32, tx_tl: u32, ty_tl: u32) -> [Tile; 4] {
        [
            self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl,     ty: ty_tl     }).unwrap(),
            self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl     }).unwrap(),
            self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl,     ty: ty_tl + 1 }).unwrap(),
            self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl + 1 }).unwrap(),
        ]
    }

    /// Box-average a 2×2 block into one tile at `mip + 1`, emit it, and
    /// recursively ingest it so even higher levels are generated.
    fn downsample_block(&mut self, block: TileBlock, emit: &mut Emitter<Item>) {
        let mip = block.coord.mip_level + 1;
        let mip_w = (self.image_width >> mip).max(1);
        let mip_h = (self.image_height >> mip).max(1);
        let tx = block.coord.tx_tl / 2;
        let ty = block.coord.ty_tl / 2;

        let all_gpu = block.tiles.iter().all(|t| t.data.is_gpu());
        let out_tile = if all_gpu {
            match gpu_downsample_block(&block, mip, tx, ty, self.tile_size, mip_w, mip_h) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("GPU mip downsample failed ({e}), falling back to CPU");
                    cpu_downsample_block(&block, mip, tx, ty, self.tile_size, mip_w, mip_h)
                }
            }
        } else {
            cpu_downsample_block(&block, mip, tx, ty, self.tile_size, mip_w, mip_h)
        };
        let device = if all_gpu { "GPU" } else { "CPU" };
        tracing::info!(
            "[pixors] mip_downsample: level={mip} tile=({tx},{ty}) {}×{} [{device}]",
            out_tile.coord.width, out_tile.coord.height,
        );

        emit.emit(Item::Tile(out_tile.clone()));

        // Recurse if the level above still has >1 tile in either dimension.
        let tiles_x = mip_w.div_ceil(self.tile_size);
        let tiles_y = mip_h.div_ceil(self.tile_size);
        if tiles_x > 1 || tiles_y > 1 {
            self.ingest(out_tile, emit);
        }
    }

    /// Flush all remaining tiles as partial blocks (edge/odd-sized images).
    fn flush_remaining(&mut self, emit: &mut Emitter<Item>) {
        // Repeatedly flush the lowest-MIP partial blocks first so their
        // outputs can form higher-level blocks in subsequent iterations.
        loop {
            // Find the lowest MIP level with remaining tiles.
            let min_mip = self.grid.keys().map(|k| k.mip_level).min();
            let Some(mip) = min_mip else { break };

            // Collect unique block TLs at this level.
            let block_tls: Vec<(u32, u32)> = self.grid.keys()
                .filter(|k| k.mip_level == mip)
                .map(|k| ((k.tx / 2) * 2, (k.ty / 2) * 2))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if block_tls.is_empty() { break; }

            for (tx_tl, ty_tl) in block_tls {
                let slots = [
                    TileGridPos { mip_level: mip, tx: tx_tl,     ty: ty_tl },
                    TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl },
                    TileGridPos { mip_level: mip, tx: tx_tl,     ty: ty_tl + 1 },
                    TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl + 1 },
                ];

                // Find any present tile to use as filler for missing slots.
                let filler_key = slots.iter().find(|k| self.grid.contains_key(k));
                let Some(filler_key) = filler_key else { continue; };
                let filler = self.grid.get(filler_key).unwrap().clone();

                // Place each tile in its correct slot, filler for absent ones.
                let tiles: [Tile; 4] = [
                    self.grid.remove(&slots[0]).unwrap_or_else(|| filler.clone()),
                    self.grid.remove(&slots[1]).unwrap_or_else(|| filler.clone()),
                    self.grid.remove(&slots[2]).unwrap_or_else(|| filler.clone()),
                    self.grid.remove(&slots[3]).unwrap_or_else(|| filler.clone()),
                ];

                let coord = TileBlockCoord { mip_level: mip, tx_tl, ty_tl };
                let block = TileBlock { coord, tiles };
                self.downsample_block(block, emit);
            }
        }
    }
}

impl CpuKernel for MipDownsampleRunner {
    fn process(&mut self, _port: u16, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("mip_downsample");
        match item {
            Item::Tile(tile) => {
                // Pass through the original tile.
                emit.emit(Item::Tile(tile.clone()));
                // Ingest it to build higher MIP levels.
                self.ingest(tile, emit);
                Ok(())
            }
            Item::TileBlock(block) => {
                self.downsample_block(block, emit);
                Ok(())
            }
            _ => Err(Error::internal("MipDownsample expected Tile or TileBlock")),
        }
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        self.flush_remaining(emit);
        Ok(())
    }
}

// ── GPU downsample ──────────────────────────────────────────────────────────

/// Downsample a 2×2 block of GPU tiles via compute shader.
/// Takes 4 GPU buffers directly — no download to CPU.
fn gpu_downsample_block(
    block: &TileBlock,
    mip: u32,
    tx: u32,
    ty: u32,
    tile_size: u32,
    mip_w: u32,
    mip_h: u32,
) -> Result<Tile, Error> {
    let ctx = gpu::try_init()
        .ok_or_else(|| Error::internal("GPU unavailable for MIP downsample"))?;
    let scheduler = ctx.scheduler();

    let gbufs: [&GpuBuffer; 4] = [
        block.tiles[0].data.as_gpu().unwrap(),
        block.tiles[1].data.as_gpu().unwrap(),
        block.tiles[2].data.as_gpu().unwrap(),
        block.tiles[3].data.as_gpu().unwrap(),
    ];

    let coord = TileCoord::new(mip, tx, ty, tile_size, mip_w, mip_h);
    let out_w = coord.width;
    let out_h = coord.height;
    let out_size = (out_w * out_h * 4) as u64;

    // Build kernel signature.
    static MIP_INPUTS: &[ResourceDeclaration] = &[
        ResourceDeclaration { name: "src_tl", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
        ResourceDeclaration { name: "src_tr", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
        ResourceDeclaration { name: "src_bl", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
        ResourceDeclaration { name: "src_br", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
    ];
    static MIP_OUTPUTS: &[ResourceDeclaration] = &[
        ResourceDeclaration { name: "dst", element: BindingElement::PixelRgba8U32, access: BindingAccess::Write },
    ];
    static MIP_PARAMS: &[ParameterDeclaration] = &[
        ParameterDeclaration { name: "out_width",  kind: ParameterType::U32 },
        ParameterDeclaration { name: "out_height", kind: ParameterType::U32 },
        ParameterDeclaration { name: "w0", kind: ParameterType::U32 },
        ParameterDeclaration { name: "h0", kind: ParameterType::U32 },
        ParameterDeclaration { name: "w1", kind: ParameterType::U32 },
        ParameterDeclaration { name: "h1", kind: ParameterType::U32 },
        ParameterDeclaration { name: "w2", kind: ParameterType::U32 },
        ParameterDeclaration { name: "h2", kind: ParameterType::U32 },
        ParameterDeclaration { name: "w3", kind: ParameterType::U32 },
        ParameterDeclaration { name: "h3", kind: ParameterType::U32 },
    ];

    let sig = KernelSignature {
        name: "cs_mip_downsample",
        entry: "cs_mip_downsample",
        inputs: MIP_INPUTS,
        outputs: MIP_OUTPUTS,
        params: MIP_PARAMS,
        workgroup: (8, 8, 1),
        dispatch: DispatchShape::PerPixel,
        class: KernelClass::Custom,
        body: MIP_DOWNSAMPLE_SPV,
    };

    // Params: out dims + 4 tile dims.
    let params_data: [u32; 10] = [
        out_w, out_h,
        block.tiles[0].coord.width, block.tiles[0].coord.height,
        block.tiles[1].coord.width, block.tiles[1].coord.height,
        block.tiles[2].coord.width, block.tiles[2].coord.height,
        block.tiles[3].coord.width, block.tiles[3].coord.height,
    ];

    struct MipKernel {
        sig: KernelSignature,
        params: [u32; 10],
    }
    impl crate::gpu::kernel::GpuKernel for MipKernel {
        fn signature(&self) -> &KernelSignature { &self.sig }
        fn write_params(&self, dst: &mut [u8]) {
            let bytes = bytemuck::cast_slice::<u32, u8>(&self.params);
            let len = bytes.len().min(dst.len());
            dst[..len].copy_from_slice(&bytes[..len]);
        }
    }

    let kernel = MipKernel { sig, params: params_data };
    let dispatch_x = out_w.div_ceil(8);
    let dispatch_y = out_h.div_ceil(8);

    let out_arc = scheduler
        .dispatch_one(&kernel, &gbufs, out_size, dispatch_x, dispatch_y)
        .map_err(|e| Error::internal(format!("mip downsample GPU: {e}")))?;

    let meta = block.tiles[0].meta;
    let out_gbuf = GpuBuffer::new(out_arc, out_size);
    Ok(Tile::new(coord, meta, Buffer::Gpu(out_gbuf)))
}

// ── CPU downsample ──────────────────────────────────────────────────────────

/// Downsample a 2×2 block of CPU tiles by box-averaging.
fn cpu_downsample_block(
    block: &TileBlock,
    mip: u32,
    tx: u32,
    ty: u32,
    tile_size: u32,
    mip_w: u32,
    mip_h: u32,
) -> Tile {
    let meta = block.tiles[0].meta;
    let coord = TileCoord::new(mip, tx, ty, tile_size, mip_w, mip_h);
    let out_w = coord.width as usize;
    let out_h = coord.height as usize;

    if out_w == 0 || out_h == 0 {
        return Tile::new(coord, meta, Buffer::cpu(vec![]));
    }

    let bpp = 4usize;
    let mut out = vec![0u8; out_w * out_h * bpp];

    let w0 = block.tiles[0].coord.width as usize;
    let h0 = block.tiles[0].coord.height as usize;
    let tiles = &block.tiles;

    out.par_chunks_mut(out_w * bpp)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..out_w {
                let avg = sample_and_average(tiles, x * 2, y * 2, w0, h0);
                let off = x * bpp;
                row[off..off + bpp].copy_from_slice(&avg);
            }
        });

    Tile::new(coord, meta, Buffer::cpu(out))
}

/// Sample a 2×2 region from the combined 4-tile block and return the average.
/// Uses (w0, h0) as the quadrant boundary (TL tile dimensions).
fn sample_and_average(tiles: &[Tile; 4], sx: usize, sy: usize, w0: usize, h0: usize) -> [u8; 4] {
    let mut sum = [0u32; 4];
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let px = sx + dx;
        let py = sy + dy;

        // Route to correct quadrant.
        let (tile_idx, lx, ly) = if py < h0 {
            if px < w0 { (0, px, py) } else { (1, px - w0, py) }
        } else {
            if px < w0 { (2, px, py - h0) } else { (3, px - w0, py - h0) }
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
        let off = (cy * tw + cx) * 4;
        if off + 4 <= data.len() {
            for c in 0..4 { sum[c] += data[off + c] as u32; }
        }
    }

    [
        (sum[0] / 4) as u8,
        (sum[1] / 4) as u8,
        (sum[2] / 4) as u8,
        (sum[3] / 4) as u8,
    ]
}
