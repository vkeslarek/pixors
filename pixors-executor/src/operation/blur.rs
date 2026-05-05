use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::data::neighborhood::Neighborhood;
use crate::data::tile::Tile;
use crate::error::Error;
use crate::gpu;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ParameterDeclaration, ParameterType, ResourceDeclaration,
};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};

use crate::debug_stopwatch;

const BLUR_SPIRV: &[u8] = include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/blur.spv"));

static BLUR_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "neighborhood",
    kind: DataKind::Neighborhood,
}];

static BLUR_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static BLUR_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(BLUR_INPUTS),
    outputs: PortGroup::Fixed(BLUR_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blur {
    pub radius: u32,
}

impl Stage for Blur {
    fn kind(&self) -> &'static str {
        "blur"
    }
    fn ports(&self) -> &'static PortSpecification {
        &BLUR_PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: true,
        }
    }
    fn device(&self) -> Device {
        Device::Gpu
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(BlurProcessor::new(self.radius)))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BlurParams {
    pub width: u32,
    pub height: u32,
    pub radius: u32,
    pub _pad: u32,
}

// ── Kernel ──────────────────────────────────────────────────────────────────

pub struct BlurProcessor {
    radius: u32,
}

impl BlurProcessor {
    pub fn new(radius: u32) -> Self {
        Self { radius }
    }
}

impl Processor for BlurProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("blur");
        let nbhd = ProcessorContext::take_neighborhood(item)?;

        let all_gpu = nbhd.tiles.iter().all(|t| t.data.is_gpu());
        if all_gpu {
            match gpu_blur_dispatch(&nbhd, self.radius) {
                Ok(tile) => {
                    ctx.emit.emit(Item::Tile(tile));
                    return Ok(());
                }
                Err(e) => tracing::warn!("GPU blur failed ({e}), falling back to CPU"),
            }
            let nbhd = download_neighborhood(nbhd)?;
            return cpu_blur_process(&nbhd, self.radius, ctx.emit);
        }

        cpu_blur_process(&nbhd, self.radius, ctx.emit)
    }
}

// ── GPU path ─────────────────────────────────────────────────────────────────

static BLUR_RES_IN: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "input",
    element: BindingElement::PixelRgba8U32,
    access: BindingAccess::Read,
}];
static BLUR_RES_OUT: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "output",
    element: BindingElement::PixelRgba8U32,
    access: BindingAccess::Write,
}];
static BLUR_PARAMS_DECL: &[ParameterDeclaration] = &[
    ParameterDeclaration {
        name: "width",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "height",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "radius",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "_pad",
        kind: ParameterType::U32,
    },
];

struct BlurGpuKernel {
    sig: KernelSignature,
    params: BlurParams,
}

impl crate::gpu::kernel::GpuKernel for BlurGpuKernel {
    fn signature(&self) -> &KernelSignature {
        &self.sig
    }
    fn write_params(&self, dst: &mut [u8]) {
        let bytes = bytemuck::bytes_of(&self.params);
        let n = bytes.len().min(dst.len());
        dst[..n].copy_from_slice(&bytes[..n]);
    }
}

fn gpu_blur_dispatch(nbhd: &Neighborhood, radius: u32) -> Result<Tile, Error> {
    let ctx =
        gpu::context::try_init().ok_or_else(|| Error::internal("GPU unavailable for blur"))?;
    let scheduler = ctx.scheduler();
    scheduler.flush();

    let mip_level = nbhd.center.mip_level;
    let r = radius >> mip_level;
    let cw = nbhd.center.width;
    let ch = nbhd.center.height;
    let cx = nbhd.center.px;
    let cy = nbhd.center.py;
    let pw = cw + 2 * r;
    let ph = ch + 2 * r;
    let ox = cx.saturating_sub(r);
    let oy = cy.saturating_sub(r);
    let bpp = 4usize;

    // Assemble padded region, downloading GPU tiles via scheduler.
    let mut assembled = vec![0u8; (pw * ph) as usize * bpp];
    for tile in &nbhd.tiles {
        let owned;
        let tile_data: &[u8] = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(gbuf) => {
                owned = scheduler.download_buffer(gbuf)?;
                &owned
            }
        };

        let tw = tile.coord.width as usize;
        let tpx = tile.coord.px;
        let tpy = tile.coord.py;
        let x0 = ox.max(tpx);
        let y0 = oy.max(tpy);
        let x1 = (ox + pw).min(tpx + tile.coord.width);
        let y1 = (oy + ph).min(tpy + tile.coord.height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }

        let copy_w = (x1 - x0) as usize;
        for abs_y in y0..y1 {
            let src = ((abs_y - tpy) as usize * tw + (x0 - tpx) as usize) * bpp;
            let dst = ((abs_y - oy) as usize * pw as usize + (x0 - ox) as usize) * bpp;
            let len = copy_w * bpp;
            if src + len <= tile_data.len() && dst + len <= assembled.len() {
                assembled[dst..dst + len].copy_from_slice(&tile_data[src..src + len]);
            }
        }
    }

    let in_gbuf = scheduler.upload_bytes(&assembled);
    let params = BlurParams {
        width: pw,
        height: ph,
        radius: r,
        _pad: 0,
    };
    let out_size = (cw * ch * 4) as u64;

    let kernel = BlurGpuKernel {
        sig: KernelSignature {
            name: "cs_blur",
            entry: "cs_blur",
            inputs: BLUR_RES_IN,
            outputs: BLUR_RES_OUT,
            params: BLUR_PARAMS_DECL,
            workgroup: (8, 8, 1),
            dispatch: DispatchShape::PerPixel,
            class: KernelClass::Custom,
            body: BLUR_SPIRV,
        },
        params,
    };
    let out_arc = scheduler
        .dispatch_one(
            &kernel,
            &[&in_gbuf],
            out_size,
            cw.div_ceil(8),
            ch.div_ceil(8),
        )
        .map_err(|e| Error::internal(format!("GPU blur: {e}")))?;

    tracing::info!(
        "[pixors] blur: GPU mip={} tile=({},{}) r={}",
        mip_level,
        nbhd.center.tx,
        nbhd.center.ty,
        r,
    );

    use crate::data::buffer::GpuBuffer;
    Ok(Tile::new(
        nbhd.center,
        nbhd.meta,
        Buffer::Gpu(GpuBuffer::new(out_arc, out_size)),
    ))
}

/// Download all GPU-backed tiles in a neighborhood to CPU.
/// Caller must NOT have pending unflushed dispatches that write to these tiles
/// (scheduler.flush() is called internally here via download_tile if needed).
fn download_neighborhood(mut nbhd: Neighborhood) -> Result<Neighborhood, Error> {
    if !nbhd.tiles.iter().any(|t| t.data.is_gpu()) {
        return Ok(nbhd);
    }
    let ctx = gpu::context::try_init()
        .ok_or_else(|| Error::internal("GPU unavailable for neighborhood download"))?;
    let scheduler = ctx.scheduler();
    scheduler.flush();
    for tile in &mut nbhd.tiles {
        if tile.data.is_gpu() {
            *tile = scheduler.download_tile(tile)?;
        }
    }
    Ok(nbhd)
}

// ── CPU path ─────────────────────────────────────────────────────────────────

fn cpu_blur_process(
    nbhd: &Neighborhood,
    radius: u32,
    emit: &mut Emitter<Item>,
) -> Result<(), Error> {
    let mip_level = nbhd.center.mip_level;
    let r = radius >> mip_level;
    let cx = nbhd.center.px;
    let cy = nbhd.center.py;
    let cw = nbhd.center.width;
    let ch = nbhd.center.height;
    let bpp = 4usize;

    if r == 0 {
        if let Some(center_tile) = nbhd.tile_at(nbhd.center.tx, nbhd.center.ty) {
            let data = center_tile.data.as_cpu_slice().unwrap();
            emit.emit(Item::Tile(Tile::new(
                nbhd.center,
                nbhd.meta,
                Buffer::cpu(data.to_vec()),
            )));
        }
        return Ok(());
    }

    let rw = (cw + 2 * r) as usize;
    let rh = (ch + 2 * r) as usize;
    let rox = cx.saturating_sub(r);
    let roy = cy.saturating_sub(r);

    let mut src = vec![0u8; rw * rh * bpp];
    for tile in &nbhd.tiles {
        let tile_data: &[u8] = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("blur CPU path received GPU tile")),
        };
        let tw = tile.coord.width as usize;
        let tpx = tile.coord.px;
        let tpy = tile.coord.py;
        let x0 = rox.max(tpx);
        let y0 = roy.max(tpy);
        let x1 = (rox + rw as u32).min(tpx + tile.coord.width);
        let y1 = (roy + rh as u32).min(tpy + tile.coord.height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }

        let copy_w = (x1 - x0) as usize;
        for abs_y in y0..y1 {
            let src_off = ((abs_y - tpy) as usize * tw + (x0 - tpx) as usize) * bpp;
            let dst_off = ((abs_y - roy) as usize * rw + (x0 - rox) as usize) * bpp;
            let len = copy_w * bpp;
            if src_off + len > tile_data.len() || dst_off + len > src.len() {
                continue;
            }
            src[dst_off..dst_off + len].copy_from_slice(&tile_data[src_off..src_off + len]);
        }
    }

    let blurred = box_blur_rgba8(&src, rw, rh, r as usize);
    let cw_u = cw as usize;
    let ch_u = ch as usize;
    let off_x = (cx - rox) as usize;
    let off_y = (cy - roy) as usize;
    let mut tile_data = Vec::with_capacity(cw_u * ch_u * bpp);
    for y in 0..ch_u {
        let row_off = ((off_y + y) * rw + off_x) * bpp;
        tile_data.extend_from_slice(&blurred[row_off..row_off + cw_u * bpp]);
    }
    emit.emit(Item::Tile(Tile::new(
        nbhd.center,
        nbhd.meta,
        Buffer::cpu(tile_data),
    )));
    Ok(())
}

fn box_blur_rgba8(data: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if r == 0 {
        return data.to_vec();
    }
    let stride = w * 4;
    let hpass = blur_axis(data, h, stride, w, 4, r);
    blur_axis(&hpass, w, 4, h, stride, r)
}

fn blur_axis(
    data: &[u8],
    lines: usize,
    line_step: usize,
    axis_len: usize,
    step: usize,
    r: usize,
) -> Vec<u8> {
    let mut dst = vec![0u8; data.len()];
    for line in 0..lines {
        let line_origin = line * line_step;
        let mut sum = [0u32; 4];
        let mut count = 0u32;
        let initial_end = r.min(axis_len - 1);
        for i in 0..=initial_end {
            let off = line_origin + i * step;
            for c in 0..4 {
                sum[c] += data[off + c] as u32;
            }
            count += 1;
        }
        for i in 0..axis_len {
            if i > 0 {
                let new_i = i + r;
                if new_i < axis_len {
                    let off = line_origin + new_i * step;
                    for c in 0..4 {
                        sum[c] += data[off + c] as u32;
                    }
                    count += 1;
                }
                if i > r {
                    let old_i = i - r - 1;
                    let off = line_origin + old_i * step;
                    for c in 0..4 {
                        sum[c] -= data[off + c] as u32;
                    }
                    count -= 1;
                }
            }
            let dst_off = line_origin + i * step;
            for c in 0..4 {
                dst[dst_off + c] = (sum[c] / count) as u8;
            }
        }
    }
    dst
}
