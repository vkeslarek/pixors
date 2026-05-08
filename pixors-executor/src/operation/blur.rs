use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::pixel::PixelFormat;
use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::data::neighborhood::{Neighborhood, NeighborhoodData};
use crate::data::tile::Tile;
use crate::error::Error;
use crate::gpu;
use crate::gpu::context::GpuContext;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ParameterDeclaration, ParameterType, ResourceDeclaration,
};
use crate::gpu::pool::GpuBuffer;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor, ProcessorContext, Stage,
    StageHints,
};

use crate::debug_stopwatch;

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
        StageHints::prefer_gpu()
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(BlurProcessor::new(self.radius)))
    }
}

// ── Processor ─────────────────────────────────────────────────────────────────

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
        if ctx.device == Device::Gpu {
            let gpu = ctx
                .gpu
                .as_ref()
                .ok_or_else(|| Error::internal("blur: GPU context missing"))?;
            gpu_blur_process(gpu, &nbhd, self.radius, ctx.emit)
        } else {
            cpu_blur_process(&nbhd, self.radius, ctx.emit)
        }
    }
}

// ── GPU path ──────────────────────────────────────────────────────────────────

const BLUR_SPV: &[u8] = include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/blur.spv"));

static BLUR_RES_IN: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "src",
    element: BindingElement::PixelRgba8U32,
    access: BindingAccess::Read,
}];
static BLUR_RES_OUT: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "dst",
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurParams {
    width: u32,
    height: u32,
    radius: u32,
    _pad: u32,
}

struct BlurGpuKernel {
    sig: KernelSignature,
    params: BlurParams,
}

impl gpu::kernel::GpuKernel for BlurGpuKernel {
    fn signature(&self) -> &KernelSignature {
        &self.sig
    }
    fn write_params(&self, dst: &mut [u8]) {
        let bytes = bytemuck::bytes_of(&self.params);
        let n = bytes.len().min(dst.len());
        dst[..n].copy_from_slice(&bytes[..n]);
    }
}

fn blur_entry(fmt: PixelFormat) -> Option<&'static str> {
    match fmt {
        PixelFormat::Rgba8 | PixelFormat::Rgb8 | PixelFormat::Gray8 | PixelFormat::GrayA8 => {
            Some("cs_blur_rgba8")
        }
        PixelFormat::Rgba16 | PixelFormat::Rgb16 => Some("cs_blur_rgba16"),
        PixelFormat::RgbaF16 | PixelFormat::RgbF16 => Some("cs_blur_rgbaf16"),
        PixelFormat::RgbaF32 | PixelFormat::RgbF32 => Some("cs_blur_rgbaf32"),
        _ => None,
    }
}

fn gpu_blur_process(
    gpu_ctx: &Arc<GpuContext>,
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
    let fmt = nbhd.meta.format;
    let bpp = fmt.bytes_per_pixel();
    let scheduler = gpu_ctx.scheduler();

    tracing::debug!(
        "[blur gpu] r={r} center=({cx},{cy}) {cw}×{ch} fmt={fmt:?} bpp={bpp} data={}",
        if nbhd.data.is_gpu() { "gpu" } else { "cpu" }
    );

    let entry = blur_entry(fmt)
        .ok_or_else(|| Error::internal(format!("blur: unsupported format {:?}", fmt)))?;

    // r=0 passthrough: extract center tile via GPU copy, no CPU round-trip.
    if r == 0 {
        let maybe_center = match &nbhd.data {
            NeighborhoodData::Cpu { tiles } => tiles
                .iter()
                .find(|t| t.coord.tx == nbhd.center.tx && t.coord.ty == nbhd.center.ty)
                .map(|t| (t.data.clone(), nbhd.meta)),
            NeighborhoodData::Gpu {
                consolidated,
                tile_infos,
            } => tile_infos
                .iter()
                .find(|i| i.px == nbhd.center.px && i.py == nbhd.center.py)
                .map(|info| {
                    let out = scheduler.allocate_buffer(info.tile_size_bytes);
                    scheduler.copy_slice(
                        consolidated.buffer(),
                        info.data_offset,
                        out.buffer(),
                        0,
                        info.tile_size_bytes,
                    );
                    (Buffer::Gpu(Arc::new(out)), nbhd.meta)
                }),
        };
        if let Some((buf, meta)) = maybe_center {
            emit.emit(Item::Tile(Tile::new(nbhd.center, meta, buf)));
        }
        return Ok(());
    }

    let pad_w = (cw + 2 * r) as usize;
    let pad_h = (ch + 2 * r) as usize;
    let orig_x = cx as i64 - r as i64;
    let orig_y = cy as i64 - r as i64;

    // GPU-Neighborhood arrives here because insert_transfers puts Upload between
    // CPU TileToNeighborhood and GPU Blur. CPU data at this point is a bug.
    let src_gbuf_arc: Arc<GpuBuffer> = match &nbhd.data {
        NeighborhoodData::Cpu { .. } => {
            return Err(Error::internal(
                "blur GPU path received CPU neighborhood — invariant violation",
            ));
        }
        NeighborhoodData::Gpu {
            consolidated,
            tile_infos,
        } => {
            let padded_size = pad_w as u64 * pad_h as u64 * bpp as u64;
            // alloc_zeroed_buffer fills via GPU clear — no CPU round-trip.
            let padded = Arc::new(scheduler.alloc_zeroed_buffer(padded_size));
            scheduler.copy_tiles_into_padded(
                consolidated.buffer(),
                tile_infos,
                padded.buffer(),
                pad_w,
                pad_h,
                orig_x,
                orig_y,
                bpp,
            );
            padded
        }
    };

    tracing::debug!(
        "[blur gpu] src buffer: {} bytes, pad={pad_w}×{pad_h}",
        src_gbuf_arc.requested_size,
    );

    let out_size = cw as u64 * ch as u64 * bpp as u64;
    let params = BlurParams {
        width: pad_w as u32,
        height: pad_h as u32,
        radius: r,
        _pad: 0,
    };

    let kernel = BlurGpuKernel {
        sig: KernelSignature {
            name: entry,
            entry,
            inputs: BLUR_RES_IN,
            outputs: BLUR_RES_OUT,
            params: BLUR_PARAMS_DECL,
            workgroup: (8, 8, 1),
            dispatch: DispatchShape::PerPixel,
            class: KernelClass::Custom,
            body: BLUR_SPV,
        },
        params,
    };

    let out_gbuf = scheduler.allocate_buffer(out_size);
    let out_gbuf = scheduler
        .dispatch_one(
            &kernel,
            &[&src_gbuf_arc],
            out_gbuf,
            cw.div_ceil(8),
            ch.div_ceil(8),
        )
        .map_err(|e| Error::internal(format!("GPU blur: {e}")))?;

    emit.emit(Item::Tile(Tile::new(
        nbhd.center,
        nbhd.meta,
        Buffer::Gpu(Arc::new(out_gbuf)),
    )));
    Ok(())
}

// ── CPU path ──────────────────────────────────────────────────────────────────

fn cpu_blur_process(
    nbhd: &Neighborhood,
    radius: u32,
    emit: &mut Emitter<Item>,
) -> Result<(), Error> {
    let tiles = match &nbhd.data {
        NeighborhoodData::Cpu { tiles } => tiles,
        NeighborhoodData::Gpu { .. } => {
            return Err(Error::internal("blur CPU path received GPU neighborhood"));
        }
    };

    let mip_level = nbhd.center.mip_level;
    let r = radius >> mip_level;
    let cx = nbhd.center.px;
    let cy = nbhd.center.py;
    let cw = nbhd.center.width;
    let ch = nbhd.center.height;
    let bpp = nbhd.meta.format.bytes_per_pixel();

    if r == 0 {
        if let Some(center_tile) = tiles
            .iter()
            .find(|t| t.coord.tx == nbhd.center.tx && t.coord.ty == nbhd.center.ty)
        {
            let data = center_tile.data.as_cpu_slice().unwrap();
            emit.emit(Item::Tile(Tile::new(
                nbhd.center,
                nbhd.meta,
                Buffer::cpu(data.to_vec()),
            )));
        }
        return Ok(());
    }

    let rox = cx.saturating_sub(r);
    let roy = cy.saturating_sub(r);
    let rw = ((cx + cw + r).min(
        tiles
            .iter()
            .map(|t| t.coord.px + t.coord.width)
            .max()
            .unwrap_or(cx + cw),
    ) - rox) as usize;
    let rh = ((cy + ch + r).min(
        tiles
            .iter()
            .map(|t| t.coord.py + t.coord.height)
            .max()
            .unwrap_or(cy + ch),
    ) - roy) as usize;

    let mut src = vec![0u8; rw * rh * bpp];
    for tile in tiles {
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

    let blurred = box_blur_format(&src, rw, rh, r as usize, nbhd.meta.format);
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

// ── CPU box blur (format-aware, operates in f32 space) ───────────────────────

#[inline]
fn decode_sample(bytes: &[u8], fmt: PixelFormat) -> f32 {
    match fmt.sample_bytes() {
        1 => bytes[0] as f32 / 255.0,
        2 if fmt.is_float() => {
            let bits = u16::from_le_bytes([bytes[0], bytes[1]]) as u32;
            let s = (bits & 0x8000) << 16;
            let em = bits & 0x7FFF;
            if em == 0 {
                return f32::from_bits(s);
            }
            let e = em >> 10;
            let m = em & 0x03FF;
            if e == 31 {
                return f32::from_bits(s | 0x7F800000 | (m << 13));
            }
            f32::from_bits(s | ((e - 15 + 127) << 23) | (m << 13))
        }
        2 => u16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 65535.0,
        4 if fmt.is_float() => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / u32::MAX as f32,
        _ => bytes[0] as f32 / 255.0,
    }
}

#[inline]
fn encode_sample(v: f32, fmt: PixelFormat, out: &mut [u8]) {
    match fmt.sample_bytes() {
        1 => out[0] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        2 if fmt.is_float() => {
            let u = v.to_bits();
            let s = (u >> 16) & 0x8000;
            let e = (u >> 23) & 0xFF;
            let m = u & 0x007FFFFF;
            let h = if e == 0 {
                s
            } else if e == 255 {
                if m == 0 {
                    s | 0x7C00
                } else {
                    s | 0x7C00 | (m >> 13)
                }
            } else {
                let i = e as i32 - 127 + 15;
                if i <= 0 {
                    s | ((0x00800000 | m) >> (1 - i) as u32 >> 13)
                } else if i >= 31 {
                    s | 0x7C00
                } else {
                    s | ((i as u32) << 10) | (m >> 13)
                }
            };
            let bytes = (h as u16).to_le_bytes();
            out[0] = bytes[0];
            out[1] = bytes[1];
        }
        2 => {
            let bytes = ((v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16).to_le_bytes();
            out[0] = bytes[0];
            out[1] = bytes[1];
        }
        4 if fmt.is_float() => {
            let bytes = v.to_le_bytes();
            out[0] = bytes[0];
            out[1] = bytes[1];
            out[2] = bytes[2];
            out[3] = bytes[3];
        }
        4 => {
            let bytes = ((v.clamp(0.0, 1.0) * u32::MAX as f32) as u32).to_le_bytes();
            out.copy_from_slice(&bytes);
        }
        _ => out[0] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    }
}

fn box_blur_format(data: &[u8], w: usize, h: usize, r: usize, fmt: PixelFormat) -> Vec<u8> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if r == 0 {
        return data.to_vec();
    }
    let ch = fmt.channel_count();
    let sb = fmt.sample_bytes();
    let bpp = ch * sb;
    let n_pixels = w * h;
    let mut f: Vec<f32> = Vec::with_capacity(n_pixels * ch);
    for p in 0..n_pixels {
        for c in 0..ch {
            let byte_off = p * bpp + c * sb;
            f.push(decode_sample(&data[byte_off..byte_off + sb], fmt));
        }
    }
    let mut hpass = vec![0f32; n_pixels * ch];
    for y in 0..h {
        for c in 0..ch {
            let mut sum = 0f32;
            let mut count = 0u32;
            let end0 = r.min(w - 1);
            for x in 0..=end0 {
                sum += f[(y * w + x) * ch + c];
                count += 1;
            }
            for x in 0..w {
                if x > 0 {
                    let nx = x + r;
                    if nx < w {
                        sum += f[(y * w + nx) * ch + c];
                        count += 1;
                    }
                    if x > r {
                        sum -= f[(y * w + x - r - 1) * ch + c];
                        count -= 1;
                    }
                }
                hpass[(y * w + x) * ch + c] = sum / count as f32;
            }
        }
    }
    let mut vpass = vec![0f32; n_pixels * ch];
    for x in 0..w {
        for c in 0..ch {
            let mut sum = 0f32;
            let mut count = 0u32;
            let end0 = r.min(h - 1);
            for y in 0..=end0 {
                sum += hpass[(y * w + x) * ch + c];
                count += 1;
            }
            for y in 0..h {
                if y > 0 {
                    let ny = y + r;
                    if ny < h {
                        sum += hpass[(ny * w + x) * ch + c];
                        count += 1;
                    }
                    if y > r {
                        sum -= hpass[((y - r - 1) * w + x) * ch + c];
                        count -= 1;
                    }
                }
                vpass[(y * w + x) * ch + c] = sum / count as f32;
            }
        }
    }
    let mut out = vec![0u8; data.len()];
    for p in 0..n_pixels {
        for c in 0..ch {
            let byte_off = p * bpp + c * sb;
            encode_sample(vpass[p * ch + c], fmt, &mut out[byte_off..byte_off + sb]);
        }
    }
    out
}
