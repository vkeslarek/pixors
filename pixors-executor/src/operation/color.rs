use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::common::color::space::ColorSpace;
use crate::common::pixel::{AlphaPolicy, PixelFormat};
use crate::data::buffer::Buffer;
use crate::data::device::Device;
use crate::error::Error;
use crate::gpu::kernel::{
    BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
    ParameterDeclaration, ParameterType, ResourceDeclaration,
};
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor, ProcessorContext, Stage,
};

use crate::debug_stopwatch;

static CC_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CC_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CC_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(CC_INPUTS),
    outputs: PortGroup::Fixed(CC_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub target_format: PixelFormat,
    pub target_color_space: ColorSpace,
    pub target_alpha: AlphaPolicy,
}

impl Stage for ColorConvert {
    fn kind(&self) -> &'static str {
        "color_convert"
    }
    fn ports(&self) -> &'static PortSpecification {
        &CC_PORTS
    }
    fn hints(&self) -> crate::stage::StageHints {
        crate::stage::StageHints::prefer_gpu()
    }
    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(ColorConvertProcessor {
            target_format: self.target_format,
            target_color_space: self.target_color_space,
            target_alpha: self.target_alpha,
        }))
    }
}

pub struct ColorConvertProcessor {
    target_format: PixelFormat,
    target_color_space: ColorSpace,
    target_alpha: AlphaPolicy,
}

impl Processor for ColorConvertProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("color_convert");
        let mut tile = ProcessorContext::take_tile(item)?;

        let src_fmt = tile.meta.format;
        let src_cs = tile.meta.color_space;
        let src_alpha = tile.meta.alpha_policy;
        let dst_fmt = self.target_format;
        let dst_cs = self.target_color_space;
        let dst_alpha = self.target_alpha;

        if src_fmt == dst_fmt && src_cs == dst_cs {
            ctx.emit.emit(Item::Tile(tile));
            return Ok(());
        }

        if ctx.device == Device::Gpu {
            let gpu = ctx
                .gpu
                .as_ref()
                .ok_or_else(|| Error::internal("GPU ColorConvert: no GPU context"))?;
            let Some(kernel) = GpuKernelSpec::select(src_fmt, dst_fmt) else {
                return Err(Error::internal(format!(
                    "GPU ColorConvert: no kernel for {src_fmt:?} -> {dst_fmt:?}"
                )));
            };

            if bytes_per_pixel(src_fmt) == bytes_per_pixel(dst_fmt)
                && let Some(ip_entry) = kernel.inplace_entry()
            {
                match gpu_dispatch_inplace(
                    tile.clone(),
                    src_fmt,
                    dst_fmt,
                    src_cs,
                    dst_cs,
                    src_alpha,
                    ip_entry,
                    gpu,
                ) {
                    Ok(result) => {
                        ctx.emit.emit(Item::Tile(result));
                        return Ok(());
                    }
                    Err(e) => tracing::warn!("GPU ColorConvert in-place failed ({e})"),
                }
            }

            // Fallback to out-of-place dispatch
            match gpu_dispatch(
                &tile, src_fmt, dst_fmt, src_cs, dst_cs, src_alpha, &kernel, gpu,
            ) {
                Ok(new_tile) => {
                    ctx.emit.emit(Item::Tile(new_tile));
                    return Ok(());
                }
                Err(e) => {
                    return Err(Error::internal(format!(
                        "GPU ColorConvert: dispatch failed for {src_fmt:?} -> {dst_fmt:?}: {e}"
                    )));
                }
            }
        }

        // ── CPU path ─────────────────────────────────────────────────────────
        let src = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("ColorConvert requires CPU tile")),
        };
        let conv = src_cs.converter_to(dst_cs)?;
        let dst_bytes = conv.convert_bytes(src, src_fmt, dst_fmt, src_alpha, dst_alpha)?;

        tile.meta.format = dst_fmt;
        tile.meta.color_space = dst_cs;
        tile.data = Buffer::cpu(dst_bytes);
        ctx.emit.emit(Item::Tile(tile));
        Ok(())
    }
}

// ── GPU path ──────────────────────────────────────────────────────────────────

const COLOR_SPV: &[u8] = include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/color.spv"));

#[derive(Copy, Clone)]
enum Precision {
    U8,
    U16,
    F16,
    F32,
}

struct GpuKernelSpec {
    entry: &'static str,
    spirv: &'static [u8],
    src_ch: u32,
    dst_ch: u32,
    src_prec: Precision,
    dst_prec: Precision,
}

fn precision(fmt: PixelFormat) -> Option<Precision> {
    match fmt {
        PixelFormat::Rgba8
        | PixelFormat::Rgb8
        | PixelFormat::Gray8
        | PixelFormat::GrayA8
        | PixelFormat::Cmyk8
        | PixelFormat::CmykA8
        | PixelFormat::YCbCr8
        | PixelFormat::Lab8 => Some(Precision::U8),

        PixelFormat::Rgba16
        | PixelFormat::Rgb16
        | PixelFormat::Gray16
        | PixelFormat::GrayA16
        | PixelFormat::Cmyk16
        | PixelFormat::CmykA16
        | PixelFormat::Lab16 => Some(Precision::U16),

        PixelFormat::RgbaF16
        | PixelFormat::RgbF16
        | PixelFormat::GrayF16
        | PixelFormat::GrayAF16
        | PixelFormat::CmykF16
        | PixelFormat::CmykAF16
        | PixelFormat::YCbCrF16 => Some(Precision::F16),

        PixelFormat::RgbaF32
        | PixelFormat::RgbF32
        | PixelFormat::GrayF32
        | PixelFormat::GrayAF32
        | PixelFormat::CmykF32
        | PixelFormat::CmykAF32
        | PixelFormat::YCbCrF32 => Some(Precision::F32),

        _ => None,
    }
}

fn channels(fmt: PixelFormat) -> Option<u32> {
    match fmt {
        // CH_RGBA = 0: 4-ch interleaved (RGBA or CMYK treated as 4-ch)
        PixelFormat::Rgba8
        | PixelFormat::Rgba16
        | PixelFormat::RgbaF16
        | PixelFormat::RgbaF32
        | PixelFormat::Cmyk8
        | PixelFormat::Cmyk16
        | PixelFormat::CmykF16
        | PixelFormat::CmykF32 => Some(0),

        // CH_RGB = 1: 3-ch interleaved
        PixelFormat::Rgb8
        | PixelFormat::Rgb16
        | PixelFormat::RgbF16
        | PixelFormat::RgbF32
        | PixelFormat::YCbCr8
        | PixelFormat::YCbCrF16
        | PixelFormat::YCbCrF32
        | PixelFormat::Lab8
        | PixelFormat::Lab16 => Some(1),

        // CH_GRAY = 2: 1-ch
        PixelFormat::Gray8 | PixelFormat::Gray16 | PixelFormat::GrayF16 | PixelFormat::GrayF32 => {
            Some(2)
        }

        // CH_GRAYA = 3: 2-ch (gray + alpha)
        PixelFormat::GrayA8
        | PixelFormat::GrayA16
        | PixelFormat::GrayAF16
        | PixelFormat::GrayAF32 => Some(3),

        // CH_CMYKA = 4: 5-ch interleaved
        PixelFormat::CmykA8
        | PixelFormat::CmykA16
        | PixelFormat::CmykAF16
        | PixelFormat::CmykAF32 => Some(4),

        _ => None,
    }
}

fn bytes_per_pixel(fmt: PixelFormat) -> Option<u64> {
    Some(fmt.bytes_per_pixel() as u64).filter(|&b| {
        b > 0 && {
            // Only return Some for formats already wired through precision()/channels()
            precision(fmt).is_some() && channels(fmt).is_some()
        }
    })
}

impl GpuKernelSpec {
    fn select(src_fmt: PixelFormat, dst_fmt: PixelFormat) -> Option<Self> {
        let src_prec = precision(src_fmt)?;
        let dst_prec = precision(dst_fmt)?;
        let src_ch = channels(src_fmt)?;
        let dst_ch = channels(dst_fmt)?;

        // src_ch == 4 → CH_CMYKA: needs decode_extra for 5th channel (alpha)
        let entry: &'static str = if src_ch == 4 {
            match (src_prec, dst_prec) {
                (Precision::U8, Precision::U8) => "cs_cc_5ch_u8_u8",
                (Precision::U8, Precision::U16) => "cs_cc_5ch_u8_u16",
                (Precision::U8, Precision::F16) => "cs_cc_5ch_u8_f16",
                (Precision::U8, Precision::F32) => "cs_cc_5ch_u8_f32",
                (Precision::U16, Precision::U8) => "cs_cc_5ch_u16_u8",
                (Precision::U16, Precision::U16) => "cs_cc_5ch_u16_u16",
                (Precision::U16, Precision::F16) => "cs_cc_5ch_u16_f16",
                (Precision::U16, Precision::F32) => "cs_cc_5ch_u16_f32",
                (Precision::F16, Precision::U8) => "cs_cc_5ch_f16_u8",
                (Precision::F16, Precision::U16) => "cs_cc_5ch_f16_u16",
                (Precision::F16, Precision::F16) => "cs_cc_5ch_f16_f16",
                (Precision::F16, Precision::F32) => "cs_cc_5ch_f16_f32",
                (Precision::F32, Precision::U8) => "cs_cc_5ch_f32_u8",
                (Precision::F32, Precision::U16) => "cs_cc_5ch_f32_u16",
                (Precision::F32, Precision::F16) => "cs_cc_5ch_f32_f16",
                (Precision::F32, Precision::F32) => "cs_cc_5ch_f32_f32",
            }
        } else {
            match (src_prec, dst_prec) {
                (Precision::U8, Precision::U8) => "cs_cc_u8_u8",
                (Precision::U8, Precision::U16) => "cs_cc_u8_u16",
                (Precision::U8, Precision::F16) => "cs_cc_u8_f16",
                (Precision::U8, Precision::F32) => "cs_cc_u8_f32",
                (Precision::U16, Precision::U8) => "cs_cc_u16_u8",
                (Precision::U16, Precision::U16) => "cs_cc_u16_u16",
                (Precision::U16, Precision::F16) => "cs_cc_u16_f16",
                (Precision::U16, Precision::F32) => "cs_cc_u16_f32",
                (Precision::F16, Precision::U8) => "cs_cc_f16_u8",
                (Precision::F16, Precision::U16) => "cs_cc_f16_u16",
                (Precision::F16, Precision::F16) => "cs_cc_f16_f16",
                (Precision::F16, Precision::F32) => "cs_cc_f16_f32",
                (Precision::F32, Precision::U8) => "cs_cc_f32_u8",
                (Precision::F32, Precision::U16) => "cs_cc_f32_u16",
                (Precision::F32, Precision::F16) => "cs_cc_f32_f16",
                (Precision::F32, Precision::F32) => "cs_cc_f32_f32",
            }
        };
        Some(Self {
            entry,
            spirv: COLOR_SPV,
            src_ch,
            dst_ch,
            src_prec,
            dst_prec,
        })
    }

    fn inplace_entry(&self) -> Option<&'static str> {
        // 5-channel (CMYKA) can't be in-place: src stride ≠ dst stride
        if self.src_ch == 4 {
            return None;
        }
        match (self.src_prec, self.dst_prec) {
            (Precision::U8, Precision::U8) => Some("cs_cc_u8_u8_ip"),
            (Precision::U16, Precision::U16) => Some("cs_cc_u16_u16_ip"),
            (Precision::F16, Precision::F16) => Some("cs_cc_f16_f16_ip"),
            (Precision::F32, Precision::F32) => Some("cs_cc_f32_f32_ip"),
            _ => None,
        }
    }
}

// ── Uniform layout ────────────────────────────────────────────────────────────

static CC_RES_IN: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "input",
    element: BindingElement::PixelRgba8U32,
    access: BindingAccess::Read,
}];
static CC_RES_OUT: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "output",
    element: BindingElement::PixelRgba8U32,
    access: BindingAccess::Write,
}];
static CC_IP_OUT: &[ResourceDeclaration] = &[ResourceDeclaration {
    name: "buf",
    element: BindingElement::PixelRgba8U32,
    access: BindingAccess::ReadWrite,
}];
static CC_PARAMS_DECL: &[ParameterDeclaration] = &[
    ParameterDeclaration {
        name: "width",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "height",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "transfer_src",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "transfer_dst",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "m00",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m01",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m02",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "_p0",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m10",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m11",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m12",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "_p1",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m20",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m21",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "m22",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "_p2",
        kind: ParameterType::F32,
    },
    ParameterDeclaration {
        name: "alpha_policy_src",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "alpha_policy_dst",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "model_transform",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "src_channels",
        kind: ParameterType::U32,
    },
    ParameterDeclaration {
        name: "dst_channels",
        kind: ParameterType::U32,
    },
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ColorConvertParams {
    width: u32,
    height: u32,
    transfer_src: u32,
    transfer_dst: u32,
    m00: f32,
    m01: f32,
    m02: f32,
    _p0: f32,
    m10: f32,
    m11: f32,
    m12: f32,
    _p1: f32,
    m20: f32,
    m21: f32,
    m22: f32,
    _p2: f32,
    alpha_policy_src: u32,
    alpha_policy_dst: u32,
    model_transform: u32,
    src_channels: u32,
    dst_channels: u32,
}

struct CcGpuKernel {
    sig: KernelSignature,
    params: ColorConvertParams,
}

impl crate::gpu::kernel::GpuKernel for CcGpuKernel {
    fn signature(&self) -> &KernelSignature {
        &self.sig
    }
    fn write_params(&self, dst: &mut [u8]) {
        let bytes = bytemuck::bytes_of(&self.params);
        let n = bytes.len().min(dst.len());
        dst[..n].copy_from_slice(&bytes[..n]);
    }
}

fn tf_u32(tf: crate::common::color::transfer::TransferFn) -> u32 {
    use crate::common::color::transfer::TransferFn::*;
    match tf {
        Linear => 0,
        SrgbGamma => 1,
        Rec709Gamma => 2,
        Gamma22 => 3,
        Gamma24 => 4,
        Gamma26 => 5,
        ProPhotoGamma => 6,
        Pq => 7,
        Hlg => 8,
    }
}

fn alpha_to_u32(a: AlphaPolicy) -> u32 {
    match a {
        AlphaPolicy::Straight => 0,
        AlphaPolicy::PremultiplyOnPack => 1,
        AlphaPolicy::OpaqueDrop => 2,
    }
}

fn gpu_dispatch(
    tile: &crate::data::tile::Tile,
    src_fmt: PixelFormat,
    dst_fmt: PixelFormat,
    src_cs: ColorSpace,
    dst_cs: ColorSpace,
    alpha: crate::common::pixel::AlphaPolicy,
    kernel_spec: &GpuKernelSpec,
    gpu: &crate::gpu::context::GpuContext,
) -> Result<crate::data::tile::Tile, Error> {
    let scheduler = gpu.scheduler();

    let in_gbuf = match &tile.data {
        Buffer::Gpu(g) => g,
        _ => return Err(Error::internal("gpu_dispatch called with non-GPU buffer")),
    };

    let cw = tile.coord.width;
    let ch = tile.coord.height;
    let bpp = bytes_per_pixel(dst_fmt).ok_or_else(|| Error::internal("unknown dst fmt"))?;
    let out_size = cw as u64 * ch as u64 * bpp;

    let conv = src_cs.converter_to(dst_cs)?;
    let mat = conv.matrix();

    let params = ColorConvertParams {
        width: cw,
        height: ch,
        transfer_src: tf_u32(src_cs.transfer()),
        transfer_dst: tf_u32(dst_cs.transfer()),
        m00: mat.0[0][0],
        m01: mat.0[0][1],
        m02: mat.0[0][2],
        _p0: 0.0,
        m10: mat.0[1][0],
        m11: mat.0[1][1],
        m12: mat.0[1][2],
        _p1: 0.0,
        m20: mat.0[2][0],
        m21: mat.0[2][1],
        m22: mat.0[2][2],
        _p2: 0.0,
        alpha_policy_src: alpha_to_u32(alpha),
        alpha_policy_dst: alpha_to_u32(AlphaPolicy::Straight),
        model_transform: src_fmt.model_transform() as u32,
        src_channels: kernel_spec.src_ch,
        dst_channels: kernel_spec.dst_ch,
    };

    let kernel = CcGpuKernel {
        sig: KernelSignature {
            name: kernel_spec.entry,
            entry: kernel_spec.entry,
            inputs: CC_RES_IN,
            outputs: CC_RES_OUT,
            params: CC_PARAMS_DECL,
            workgroup: (8, 8, 1),
            dispatch: DispatchShape::PerPixel,
            class: KernelClass::Custom,
            body: kernel_spec.spirv,
        },
        params,
    };

    let out_gbuf = scheduler.allocate_buffer(out_size);

    let out_gbuf = scheduler
        .dispatch_one(
            &kernel,
            &[in_gbuf],
            out_gbuf,
            cw.div_ceil(8),
            ch.div_ceil(8),
        )
        .map_err(|e| Error::internal(format!("GPU color convert: {e}")))?;

    let mut new_tile = tile.clone();
    new_tile.meta.format = dst_fmt;
    new_tile.meta.color_space = dst_cs;
    new_tile.data = Buffer::Gpu(Arc::new(out_gbuf));
    Ok(new_tile)
}

/// In-place GPU color convert — reuses the input buffer as output.
/// Only used when source and destination precisions are the same.
fn gpu_dispatch_inplace(
    tile: crate::data::tile::Tile,
    src_fmt: PixelFormat,
    dst_fmt: PixelFormat,
    src_cs: ColorSpace,
    dst_cs: ColorSpace,
    alpha: crate::common::pixel::AlphaPolicy,
    ip_entry: &'static str,
    gpu: &crate::gpu::context::GpuContext,
) -> Result<crate::data::tile::Tile, Error> {
    let scheduler = gpu.scheduler();

    let cw = tile.coord.width;
    let ch = tile.coord.height;
    let coord = tile.coord;
    let mut meta = tile.meta;
    let gbuf = match tile.data {
        Buffer::Gpu(g) => g,
        _ => {
            return Err(Error::internal(
                "gpu_dispatch_inplace called with non-GPU buffer",
            ));
        }
    };

    let conv = src_cs.converter_to(dst_cs)?;
    let mat = conv.matrix();

    let params = ColorConvertParams {
        width: cw,
        height: ch,
        transfer_src: tf_u32(src_cs.transfer()),
        transfer_dst: tf_u32(dst_cs.transfer()),
        m00: mat.0[0][0],
        m01: mat.0[0][1],
        m02: mat.0[0][2],
        _p0: 0.0,
        m10: mat.0[1][0],
        m11: mat.0[1][1],
        m12: mat.0[1][2],
        _p1: 0.0,
        m20: mat.0[2][0],
        m21: mat.0[2][1],
        m22: mat.0[2][2],
        _p2: 0.0,
        alpha_policy_src: alpha_to_u32(alpha),
        alpha_policy_dst: alpha_to_u32(AlphaPolicy::Straight),
        model_transform: src_fmt.model_transform() as u32,
        src_channels: channels(src_fmt).unwrap_or(0),
        dst_channels: channels(dst_fmt).unwrap_or(0),
    };

    let exclusive = match Arc::try_unwrap(gbuf) {
        Ok(owned) => owned,
        Err(arc) => scheduler
            .deep_copy_buffer(&arc)
            .map_err(|e| Error::internal(format!("GPU color convert in-place deep_copy: {e}")))?,
    };
    let exclusive = Arc::new(exclusive);

    let ip_kernel = CcGpuKernel {
        sig: KernelSignature {
            name: ip_entry,
            entry: ip_entry,
            inputs: &[],
            outputs: CC_IP_OUT,
            params: CC_PARAMS_DECL,
            workgroup: (8, 8, 1),
            dispatch: DispatchShape::PerPixel,
            class: KernelClass::Custom,
            body: COLOR_SPV,
        },
        params,
    };

    scheduler
        .dispatch_inplace(&ip_kernel, &exclusive, cw.div_ceil(8), ch.div_ceil(8))
        .map_err(|e| Error::internal(format!("GPU color convert in-place: {e}")))?;

    meta.format = dst_fmt;
    meta.color_space = dst_cs;
    Ok(crate::data::tile::Tile::new(
        coord,
        meta,
        Buffer::Gpu(exclusive),
    ))
}
