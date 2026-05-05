use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::error::Error;
use crate::gpu;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ParameterDeclaration, ParameterType, ResourceDeclaration,
};
use crate::graph::item::Item;
use crate::model::color::space::ColorSpace;
use crate::model::pixel::PixelFormat;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};

use crate::debug_stopwatch;

static CC_INPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];
static CC_OUTPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];
static CC_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(CC_INPUTS),
    outputs: PortGroup::Fixed(CC_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub target_format: PixelFormat,
    pub target_color_space: ColorSpace,
}

impl Stage for ColorConvert {
    fn kind(&self) -> &'static str { "color_convert" }
    fn ports(&self) -> &'static PortSpecification { &CC_PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadTransform, prefers_gpu: true }
    }
    fn device(&self) -> Device { Device::Either }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(ColorConvertProcessor {
            target_format: self.target_format,
            target_color_space: self.target_color_space,
        }))
    }
}

pub struct ColorConvertProcessor {
    target_format: PixelFormat,
    target_color_space: ColorSpace,
}

macro_rules! convert_via {
    ($conv:expr, $src:expr, $src_ty:ty, $dst_ty:ty, $alpha:expr) => {{
        let pixels: &[$src_ty] = bytemuck::cast_slice($src);
        let result: Vec<$dst_ty> = $conv.convert_pixels(pixels, $alpha);
        bytemuck::cast_slice(&result).to_vec()
    }};
}

impl Processor for ColorConvertProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("color_convert");
        let mut tile = ProcessorContext::take_tile(item)?;

        let src_fmt = tile.meta.format;
        let src_cs = tile.meta.color_space;
        let alpha = tile.meta.alpha_policy;
        let dst_fmt = self.target_format;
        let dst_cs = self.target_color_space;

        if src_fmt == dst_fmt && src_cs == dst_cs {
            ctx.emit.emit(Item::Tile(tile));
            return Ok(());
        }

        if tile.data.is_gpu() {
            if let Some(kernel) = GpuKernelSpec::select(src_fmt, dst_fmt) {
                match gpu_dispatch(&tile, src_fmt, dst_fmt, src_cs, dst_cs, alpha, &kernel) {
                    Ok(new_tile) => {
                        ctx.emit.emit(Item::Tile(new_tile));
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("GPU ColorConvert failed ({e}), falling back to CPU");
                        tile = download_tile(tile)?;
                    }
                }
            } else {
                tracing::warn!(
                    "GPU ColorConvert: no kernel for {src_fmt:?} -> {dst_fmt:?}, falling back to CPU"
                );
                tile = download_tile(tile)?;
            }
        }

        // ── CPU path ─────────────────────────────────────────────────────────
        let src = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("ColorConvert requires CPU tile")),
        };
        let conv = src_cs.converter_to(dst_cs)?;

        use crate::model::pixel::{Rgb, Rgba};
        use half::f16;

        let dst_bytes: Vec<u8> = match (src_fmt, dst_fmt) {
            (PixelFormat::Rgba8, PixelFormat::Rgba8) => {
                convert_via!(conv, src, [u8; 4], [u8; 4], alpha)
            }
            (PixelFormat::Rgb8, PixelFormat::Rgba8) => {
                let src_pixels: &[[u8; 3]] = bytemuck::cast_slice(src);
                let mut out = Vec::with_capacity(src_pixels.len());
                for px in src_pixels {
                    out.push([px[0], px[1], px[2], 255u8]);
                }
                convert_via!(conv, &out, [u8; 4], [u8; 4], alpha)
            }
            (PixelFormat::Gray8, PixelFormat::Rgba8) => {
                let mut out = Vec::with_capacity(src.len());
                for &v in src {
                    out.extend_from_slice(&[v, v, v, 255u8]);
                }
                convert_via!(conv, &out, [u8; 4], [u8; 4], alpha)
            }
            (PixelFormat::GrayA8, PixelFormat::Rgba8) => {
                let mut out = Vec::with_capacity(src.len() * 2);
                for chunk in src.chunks_exact(2) {
                    out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
                }
                convert_via!(conv, &out, [u8; 4], [u8; 4], alpha)
            }
            (PixelFormat::Rgba16, PixelFormat::Rgba8) => {
                convert_via!(conv, src, [u16; 4], [u8; 4], alpha)
            }
            (PixelFormat::Rgb16, PixelFormat::Rgba8) => {
                let src_pixels: &[[u16; 3]] = bytemuck::cast_slice(src);
                let mut out = Vec::with_capacity(src_pixels.len());
                for px in src_pixels {
                    out.push([px[0], px[1], px[2], 65535u16]);
                }
                convert_via!(conv, &out, [u16; 4], [u8; 4], alpha)
            }
            (PixelFormat::RgbaF16, PixelFormat::Rgba8) => {
                convert_via!(conv, src, Rgba<f16>, [u8; 4], alpha)
            }
            (PixelFormat::RgbaF32, PixelFormat::Rgba8) => {
                convert_via!(conv, src, Rgba<f32>, [u8; 4], alpha)
            }
            (PixelFormat::RgbF16, PixelFormat::Rgba8) => {
                let src_pixels: &[Rgb<f16>] = bytemuck::cast_slice(src);
                let mut out = Vec::with_capacity(src_pixels.len());
                for px in src_pixels {
                    out.push(Rgba { r: px.r, g: px.g, b: px.b, a: f16::ONE });
                }
                convert_via!(conv, &out, Rgba<f16>, [u8; 4], alpha)
            }
            (PixelFormat::RgbF32, PixelFormat::Rgba8) => {
                let src_pixels: &[Rgb<f32>] = bytemuck::cast_slice(src);
                let mut out = Vec::with_capacity(src_pixels.len());
                for px in src_pixels {
                    out.push(Rgba { r: px.r, g: px.g, b: px.b, a: 1.0f32 });
                }
                convert_via!(conv, &out, Rgba<f32>, [u8; 4], alpha)
            }
            _ => {
                return Err(Error::internal(format!(
                    "ColorConvert: unsupported conversion {:?} -> {:?}",
                    src_fmt, dst_fmt
                )));
            }
        };

        tile.meta.format = dst_fmt;
        tile.meta.color_space = dst_cs;
        tile.data = Buffer::cpu(dst_bytes);
        ctx.emit.emit(Item::Tile(tile));
        Ok(())
    }
}

// ── GPU path ──────────────────────────────────────────────────────────────────

// SPIR-V binaries for each src×dst precision pair.
macro_rules! spirv {
    ($name:literal) => {
        include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/", $name, ".spv"))
    };
}

const SPV_U8_U8:   &[u8] = spirv!("cc_u8_u8");
const SPV_U8_U16:  &[u8] = spirv!("cc_u8_u16");
const SPV_U8_F16:  &[u8] = spirv!("cc_u8_f16");
const SPV_U8_F32:  &[u8] = spirv!("cc_u8_f32");
const SPV_U16_U8:  &[u8] = spirv!("cc_u16_u8");
const SPV_U16_U16: &[u8] = spirv!("cc_u16_u16");
const SPV_U16_F16: &[u8] = spirv!("cc_u16_f16");
const SPV_U16_F32: &[u8] = spirv!("cc_u16_f32");
const SPV_F16_U8:  &[u8] = spirv!("cc_f16_u8");
const SPV_F16_U16: &[u8] = spirv!("cc_f16_u16");
const SPV_F16_F16: &[u8] = spirv!("cc_f16_f16");
const SPV_F16_F32: &[u8] = spirv!("cc_f16_f32");
const SPV_F32_U8:  &[u8] = spirv!("cc_f32_u8");
const SPV_F32_U16: &[u8] = spirv!("cc_f32_u16");
const SPV_F32_F16: &[u8] = spirv!("cc_f32_f16");
const SPV_F32_F32: &[u8] = spirv!("cc_f32_f32");

#[derive(Copy, Clone)]
enum Precision { U8, U16, F16, F32 }

struct GpuKernelSpec {
    entry:      &'static str,
    spirv:      &'static [u8],
    src_ch:     u32,
    dst_ch:     u32,
    src_prec:   Precision,
    dst_prec:   Precision,
}

fn precision(fmt: PixelFormat) -> Option<Precision> {
    match fmt {
        PixelFormat::Rgba8  | PixelFormat::Rgb8 |
        PixelFormat::Gray8  | PixelFormat::GrayA8  => Some(Precision::U8),
        PixelFormat::Rgba16 | PixelFormat::Rgb16   => Some(Precision::U16),
        PixelFormat::RgbaF16| PixelFormat::RgbF16  => Some(Precision::F16),
        PixelFormat::RgbaF32| PixelFormat::RgbF32  => Some(Precision::F32),
        _ => None,
    }
}

fn channels(fmt: PixelFormat) -> Option<u32> {
    match fmt {
        PixelFormat::Rgba8 | PixelFormat::Rgba16 |
        PixelFormat::RgbaF16 | PixelFormat::RgbaF32 => Some(0), // CH_RGBA
        PixelFormat::Rgb8  | PixelFormat::Rgb16  |
        PixelFormat::RgbF16  | PixelFormat::RgbF32  => Some(1), // CH_RGB
        PixelFormat::Gray8   => Some(2), // CH_GRAY
        PixelFormat::GrayA8  => Some(3), // CH_GRAYA
        _ => None,
    }
}

fn bytes_per_pixel(fmt: PixelFormat) -> Option<u64> {
    match fmt {
        PixelFormat::Rgba8   => Some(4),
        PixelFormat::Rgb8    => Some(3),
        PixelFormat::Gray8   => Some(1),
        PixelFormat::GrayA8  => Some(2),
        PixelFormat::Rgba16  => Some(8),
        PixelFormat::Rgb16   => Some(6),
        PixelFormat::RgbaF16 => Some(8),
        PixelFormat::RgbF16  => Some(6),
        PixelFormat::RgbaF32 => Some(16),
        PixelFormat::RgbF32  => Some(12),
        _ => None,
    }
}

impl GpuKernelSpec {
    fn select(src_fmt: PixelFormat, dst_fmt: PixelFormat) -> Option<Self> {
        let src_prec = precision(src_fmt)?;
        let dst_prec = precision(dst_fmt)?;
        let src_ch   = channels(src_fmt)?;
        let dst_ch   = channels(dst_fmt)?;

        let (entry, spirv): (&'static str, &'static [u8]) = match (src_prec, dst_prec) {
            (Precision::U8,  Precision::U8)  => ("cs_cc_u8_u8",   SPV_U8_U8),
            (Precision::U8,  Precision::U16) => ("cs_cc_u8_u16",  SPV_U8_U16),
            (Precision::U8,  Precision::F16) => ("cs_cc_u8_f16",  SPV_U8_F16),
            (Precision::U8,  Precision::F32) => ("cs_cc_u8_f32",  SPV_U8_F32),
            (Precision::U16, Precision::U8)  => ("cs_cc_u16_u8",  SPV_U16_U8),
            (Precision::U16, Precision::U16) => ("cs_cc_u16_u16", SPV_U16_U16),
            (Precision::U16, Precision::F16) => ("cs_cc_u16_f16", SPV_U16_F16),
            (Precision::U16, Precision::F32) => ("cs_cc_u16_f32", SPV_U16_F32),
            (Precision::F16, Precision::U8)  => ("cs_cc_f16_u8",  SPV_F16_U8),
            (Precision::F16, Precision::U16) => ("cs_cc_f16_u16", SPV_F16_U16),
            (Precision::F16, Precision::F16) => ("cs_cc_f16_f16", SPV_F16_F16),
            (Precision::F16, Precision::F32) => ("cs_cc_f16_f32", SPV_F16_F32),
            (Precision::F32, Precision::U8)  => ("cs_cc_f32_u8",  SPV_F32_U8),
            (Precision::F32, Precision::U16) => ("cs_cc_f32_u16", SPV_F32_U16),
            (Precision::F32, Precision::F16) => ("cs_cc_f32_f16", SPV_F32_F16),
            (Precision::F32, Precision::F32) => ("cs_cc_f32_f32", SPV_F32_F32),
        };
        Some(Self { entry, spirv, src_ch, dst_ch, src_prec, dst_prec })
    }
}

// ── Uniform layout ────────────────────────────────────────────────────────────

static CC_RES_IN: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "input", element: BindingElement::PixelRgba8U32, access: BindingAccess::Read,
}];
static CC_RES_OUT: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "output", element: BindingElement::PixelRgba8U32, access: BindingAccess::Write,
}];
static CC_PARAMS_DECL: &[ParameterDeclaration] = &[
    ParameterDeclaration { name: "width",        kind: ParameterType::U32 },
    ParameterDeclaration { name: "height",       kind: ParameterType::U32 },
    ParameterDeclaration { name: "transfer_src", kind: ParameterType::U32 },
    ParameterDeclaration { name: "transfer_dst", kind: ParameterType::U32 },
    ParameterDeclaration { name: "m00",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m01",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m02",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "_p0",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m10",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m11",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m12",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "_p1",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m20",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m21",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "m22",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "_p2",  kind: ParameterType::F32 },
    ParameterDeclaration { name: "alpha_policy",  kind: ParameterType::U32 },
    ParameterDeclaration { name: "src_channels",  kind: ParameterType::U32 },
    ParameterDeclaration { name: "dst_channels",  kind: ParameterType::U32 },
    ParameterDeclaration { name: "_p3",           kind: ParameterType::U32 },
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ColorConvertParams {
    width: u32, height: u32, transfer_src: u32, transfer_dst: u32,
    m00: f32, m01: f32, m02: f32, _p0: f32,
    m10: f32, m11: f32, m12: f32, _p1: f32,
    m20: f32, m21: f32, m22: f32, _p2: f32,
    alpha_policy: u32, src_channels: u32, dst_channels: u32, _p3: u32,
}

struct CcGpuKernel { sig: KernelSignature, params: ColorConvertParams }

impl crate::gpu::kernel::GpuKernel for CcGpuKernel {
    fn signature(&self) -> &KernelSignature { &self.sig }
    fn write_params(&self, dst: &mut [u8]) {
        let bytes = bytemuck::bytes_of(&self.params);
        let n = bytes.len().min(dst.len());
        dst[..n].copy_from_slice(&bytes[..n]);
    }
}

fn tf_u32(tf: crate::model::color::transfer::TransferFn) -> u32 {
    use crate::model::color::transfer::TransferFn::*;
    match tf {
        Linear => 0, SrgbGamma => 1, Rec709Gamma => 2,
        Gamma22 => 3, Gamma24 => 4, Gamma26 => 5,
        ProPhotoGamma => 6, Pq => 7, Hlg => 8,
    }
}

fn gpu_dispatch(
    tile: &crate::data::tile::Tile,
    src_fmt: PixelFormat, dst_fmt: PixelFormat,
    src_cs: ColorSpace, dst_cs: ColorSpace,
    alpha: crate::model::pixel::AlphaPolicy,
    kernel_spec: &GpuKernelSpec,
) -> Result<crate::data::tile::Tile, Error> {
    let ctx = gpu::context::try_init().ok_or_else(|| Error::internal("GPU unavailable"))?;
    let scheduler = ctx.scheduler();
    scheduler.flush();

    let in_gbuf = match &tile.data {
        Buffer::Gpu(g) => g,
        _ => return Err(Error::internal("gpu_dispatch called with non-GPU buffer")),
    };

    let cw = tile.coord.width;
    let ch = tile.coord.height;
    let bpp = bytes_per_pixel(dst_fmt).ok_or_else(|| Error::internal("unknown dst fmt"))?;
    let out_size = cw as u64 * ch as u64 * bpp;

    let conv = src_cs.converter_to(dst_cs)?;
    let mat  = conv.matrix();

    let params = ColorConvertParams {
        width: cw, height: ch,
        transfer_src: tf_u32(src_cs.transfer()),
        transfer_dst: tf_u32(dst_cs.transfer()),
        m00: mat.0[0][0], m01: mat.0[0][1], m02: mat.0[0][2], _p0: 0.0,
        m10: mat.0[1][0], m11: mat.0[1][1], m12: mat.0[1][2], _p1: 0.0,
        m20: mat.0[2][0], m21: mat.0[2][1], m22: mat.0[2][2], _p2: 0.0,
        alpha_policy: match alpha {
            crate::model::pixel::AlphaPolicy::Straight => 0,
            _ => 1,
        },
        src_channels: kernel_spec.src_ch,
        dst_channels: kernel_spec.dst_ch,
        _p3: 0,
    };

    let kernel = CcGpuKernel {
        sig: KernelSignature {
            name:      kernel_spec.entry,
            entry:     kernel_spec.entry,
            inputs:    CC_RES_IN,
            outputs:   CC_RES_OUT,
            params:    CC_PARAMS_DECL,
            workgroup: (8, 8, 1),
            dispatch:  DispatchShape::PerPixel,
            class:     KernelClass::Custom,
            body:      kernel_spec.spirv,
        },
        params,
    };

    let out_arc = scheduler.dispatch_one(
        &kernel, &[in_gbuf], out_size,
        cw.div_ceil(8), ch.div_ceil(8),
    ).map_err(|e| Error::internal(format!("GPU color convert: {e}")))?;

    let mut new_tile = tile.clone();
    new_tile.meta.format = dst_fmt;
    new_tile.meta.color_space = dst_cs;
    new_tile.data = Buffer::Gpu(crate::data::buffer::GpuBuffer::new(out_arc, out_size));
    Ok(new_tile)
}

fn download_tile(mut tile: crate::data::tile::Tile) -> Result<crate::data::tile::Tile, Error> {
    let ctx = gpu::context::try_init()
        .ok_or_else(|| Error::internal("GPU unavailable for tile download"))?;
    let scheduler = ctx.scheduler();
    scheduler.flush();
    tile = scheduler.download_tile(&tile)?;
    Ok(tile)
}
