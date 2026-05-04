use serde::{Deserialize, Serialize};

use crate::data::{Buffer, GpuBuffer, Tile, TileBlock, TileCoord};
use crate::debug_stopwatch;
use crate::error::Error;
use crate::gpu;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ResourceDeclaration,
};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};

static IN: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static OUT: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static PORTS: PortSpec = PortSpec { inputs: IN, outputs: OUT };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileBlockToTile {
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
}

impl Stage for TileBlockToTile {
    fn kind(&self) -> &'static str { "tile_block_to_tile" }

    fn ports(&self) -> &'static PortSpec { &PORTS }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(TileBlockToTileRunner::new(
            self.image_width,
            self.image_height,
            self.tile_size,
        )))
    }
}

pub struct TileBlockToTileRunner {
    image_width: u32,
    image_height: u32,
    tile_size: u32,
}

impl TileBlockToTileRunner {
    pub fn new(image_width: u32, image_height: u32, tile_size: u32) -> Self {
        Self { image_width, image_height, tile_size }
    }
}

impl CpuKernel for TileBlockToTileRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("tile_block_to_tile");
        match item {
            Item::Tile(tile) => {
                emit.emit(Item::Tile(tile));
                Ok(())
            }
            Item::TileBlock(block) => {
                let mip = block.coord.mip_level + 1;
                let all_gpu = block.tiles.iter().all(|t| t.data.is_gpu());
                if all_gpu {
                    gpu_downsample(block, mip, emit)
                } else {
                    cpu_downsample(block, mip, self.tile_size, self.image_width >> mip, self.image_height >> mip, emit)
                }
            }
            _ => Err(Error::internal("TileBlockToTile expected Tile or TileBlock")),
        }
    }
}

fn cpu_downsample(
    block: TileBlock,
    _mip: u32,
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
    let coord = TileCoord::new(block.coord.mip_level + 1, tx_tl, ty_tl, tile_size, mip_w, mip_h);
    emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(out))));
    Ok(())
}

fn sample_2x2(tiles: &[Tile; 4], x: usize, y: usize) -> [[u8; 4]; 4] {
    let mut out = [[0u8; 4]; 4];
    let positions = [(0usize, 0usize), (1, 0), (0, 1), (1, 1)];
    for (i, (dx, dy)) in positions.iter().enumerate() {
        let sx = x + dx;
        let sy = y + dy;
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

fn gpu_downsample(
    block: TileBlock,
    _mip: u32,
    emit: &mut Emitter<Item>,
) -> Result<(), Error> {
    let ctx = gpu::try_init()
        .ok_or_else(|| Error::internal("GPU unavailable for MIP downsample"))?;
    let scheduler = ctx.scheduler();

    let gbufs: [&GpuBuffer; 4] = [
        block.tiles[0].data.as_gpu().unwrap(),
        block.tiles[1].data.as_gpu().unwrap(),
        block.tiles[2].data.as_gpu().unwrap(),
        block.tiles[3].data.as_gpu().unwrap(),
    ];

    let out_w = block.tiles.iter().map(|t| t.coord.width).max().unwrap_or(0);
    let out_h = block.tiles.iter().map(|t| t.coord.height).max().unwrap_or(0);
    let out_size = (out_w * out_h * 4) as u64;

    static MIP_INPUTS: &[ResourceDeclaration] = &[
        ResourceDeclaration { name: "src00", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
        ResourceDeclaration { name: "src01", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
        ResourceDeclaration { name: "src10", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
        ResourceDeclaration { name: "src11", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read },
    ];
    static MIP_OUTPUTS: &[ResourceDeclaration] = &[
        ResourceDeclaration { name: "output", element: BindingElement::PixelRgba8U32, access: BindingAccess::Write },
    ];

    let sig = KernelSignature {
        name: "cs_mip_downsample",
        entry: "cs_mip_downsample",
        inputs: MIP_INPUTS,
        outputs: MIP_OUTPUTS,
        params: &[],
        workgroup: (8, 8, 1),
        dispatch: DispatchShape::PerPixel,
        class: KernelClass::Custom,
        body: &[],
    };

    let meta = block.tiles[0].meta;
    let tile_size = block.tiles[0].coord.width.max(block.tiles[0].coord.height);
    let tx_tl = block.coord.tx_tl / 2;
    let ty_tl = block.coord.ty_tl / 2;
    let coord = TileCoord::new(block.coord.mip_level + 1, tx_tl, ty_tl, tile_size, tile_size, tile_size);

    struct NoParamsKernel { sig: KernelSignature }
    impl crate::gpu::kernel::GpuKernel for NoParamsKernel {
        fn signature(&self) -> &KernelSignature { &self.sig }
        fn write_params(&self, _dst: &mut [u8]) {}
    }

    let kernel = NoParamsKernel { sig };
    let dispatch_x = out_w.div_ceil(8);
    let dispatch_y = out_h.div_ceil(8);

    let out_arc = scheduler
        .dispatch_one(&kernel, &gbufs, out_size, dispatch_x, dispatch_y)
        .map_err(|e| Error::internal(format!("mip downsample GPU: {e}")))?;

    let out_gbuf = GpuBuffer::new(out_arc, out_size);
    emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::Gpu(out_gbuf))));
    Ok(())
}
