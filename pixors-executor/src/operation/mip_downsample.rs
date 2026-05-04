use serde::{Deserialize, Serialize};

use crate::data::{Buffer, Device, Tile, TileBlock, TileCoord};
use crate::debug_stopwatch;
use crate::error::Error;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ResourceDeclaration,
};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, CpuKernel, DataKind, GpuInputBinding, GpuKernelDescriptor, PortDecl, PortSpec,
    Stage, StageHints,
};

const MIP_DOWNSAMPLE_SPV: &[u8] =
    include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/mip_downsample.spv"));

static IN: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static OUT: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static PORTS: PortSpec = PortSpec { inputs: IN, outputs: OUT };

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
        StageHints { buffer_access: BufferAccess::ReadTransform, prefers_gpu: true }
    }

    fn device(&self) -> Device { Device::Either }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(MipDownsampleRunner::new(
            self.image_width, self.image_height, self.tile_size,
        )))
    }

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        let image_width = self.image_width;
        let image_height = self.image_height;
        let tile_size = self.tile_size;
        Some(GpuKernelDescriptor {
            spirv: MIP_DOWNSAMPLE_SPV,
            entry_point: "cs_mip_downsample",
            input_binding: GpuInputBinding::Tile,
            workgroup: (8, 8),
            param_size: 0,
            write_params: None,
        })
    }
}

pub struct MipDownsampleRunner {
    image_width: u32,
    image_height: u32,
    tile_size: u32,
}

impl MipDownsampleRunner {
    pub fn new(image_width: u32, image_height: u32, tile_size: u32) -> Self {
        Self { image_width, image_height, tile_size }
    }
}

impl CpuKernel for MipDownsampleRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("mip_downsample");
        match item {
            Item::Tile(tile) => {
                emit.emit(Item::Tile(tile));
                Ok(())
            }
            Item::TileBlock(block) => {
                let mip = block.coord.mip_level + 1;
                cpu_downsample(
                    block, mip, self.tile_size,
                    self.image_width >> mip,
                    self.image_height >> mip,
                    emit,
                )
            }
            _ => Err(Error::internal("MipDownsample expected Tile or TileBlock")),
        }
    }
}

fn cpu_downsample(
    block: TileBlock,
    mip: u32,
    tile_size: u32,
    mip_w: u32,
    mip_h: u32,
    emit: &mut Emitter<Item>,
) -> Result<(), Error> {
    let meta = block.tiles[0].meta;
    let bpp = meta.format.bytes_per_pixel() as usize;

    let tw = block.tiles.iter().map(|t| t.coord.width).max().unwrap_or(tile_size);
    let th = block.tiles.iter().map(|t| t.coord.height).max().unwrap_or(tile_size);
    let out_w = tw.min(tile_size);
    let out_h = th.min(tile_size);

    let mut out = vec![0u8; out_w as usize * out_h as usize * bpp];

    for y in 0..out_h as usize {
        for x in 0..out_w as usize {
            let sx = x * 2;
            let sy = y * 2;
            let pixels = sample_2x2(&block.tiles, sx, sy);
            let avg = avg_rgba8(&pixels);
            let off = (y * out_w as usize + x) * bpp;
            if off + bpp <= out.len() {
                out[off..off + bpp].copy_from_slice(&avg);
            }
        }
    }

    let tx_tl = block.coord.tx_tl / 2;
    let ty_tl = block.coord.ty_tl / 2;
    let coord = TileCoord::new(mip, tx_tl, ty_tl, tile_size, mip_w, mip_h);
    emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(out))));
    Ok(())
}

fn sample_2x2(tiles: &[Tile; 4], x: usize, y: usize) -> [[u8; 4]; 4] {
    let mut out = [[0u8; 4]; 4];
    let indices = [0usize, 1, 2, 3];
    let dx = [0usize, 1, 0, 1];
    let dy = [0usize, 0, 1, 1];
    for i in indices {
        let sx = x + dx[i];
        let sy = y + dy[i];
        let tile = &tiles[i];
        let w = tile.coord.width as usize;
        let h = tile.coord.height as usize;
        let cx = sx.min(w.saturating_sub(1));
        let cy = sy.min(h.saturating_sub(1));
        let data: &[u8] = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => &[],
        };
        let bpp = 4;
        let off = (cy * w + cx) * bpp;
        if off + bpp <= data.len() {
            out[i].copy_from_slice(&data[off..off + bpp]);
        }
    }
    out
}

fn avg_rgba8(pixels: &[[u8; 4]; 4]) -> [u8; 4] {
    let mut sum = [0u32; 4];
    for p in pixels {
        for c in 0..4 {
            sum[c] += p[c] as u32;
        }
    }
    [
        (sum[0] / 4) as u8,
        (sum[1] / 4) as u8,
        (sum[2] / 4) as u8,
        (sum[3] / 4) as u8,
    ]
}
