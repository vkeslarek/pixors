use std::sync::Arc;

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::data::buffer::Buffer;
use pixors_engine::data::device::Device;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
    StageHints,
};

use pixors_engine::debug_stopwatch;
use pixors_shader::kernel::color::ColorConvertParams;

static CC_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CC_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CC_PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Fixed(CC_INPUTS),
    outputs: PortGroup::Fixed(CC_OUTPUTS),
};

#[derive(Debug, Clone)]
pub struct ColorConvert {
    pub target_format: PixelFormat,
    pub target_color_space: ColorSpace,
    pub target_alpha: AlphaPolicy,
}

impl Processor for ColorConvert {
    fn kind(&self) -> &'static str {
        "color_convert"
    }
    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &CC_PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints::prefer_gpu()
    }

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

            let new_tile = gpu_dispatch(
                &tile,
                src_fmt,
                dst_fmt,
                src_cs,
                dst_cs,
                src_alpha,
                gpu,
            )?;
            ctx.emit.emit(Item::Tile(new_tile));
            return Ok(());
        }

        // ── CPU path ─────────────────────────────────────────────────────────
        let src = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("ColorConvert requires CPU tile")),
        };
        let conv = pixors_engine::common::color::conversion::ColorConversion::new(src_cs, dst_cs)?;
        let dst_bytes = conv.convert_bytes(src, src_fmt, dst_fmt, src_alpha, dst_alpha)?;

        tile.meta.format = dst_fmt;
        tile.meta.color_space = dst_cs;
        tile.data = Buffer::cpu(dst_bytes);
        ctx.emit.emit(Item::Tile(tile));
        Ok(())
    }
}

// ── GPU path ──────────────────────────────────────────────────────────────────

enum Precision {
    U8,
    U16,
    F16,
    F32,
}

fn precision(fmt: PixelFormat) -> Option<Precision> {
    match fmt {
        PixelFormat::Rgba8 | PixelFormat::Rgb8 | PixelFormat::Gray8 | PixelFormat::GrayA8
        | PixelFormat::Cmyk8 | PixelFormat::CmykA8 | PixelFormat::YCbCr8 | PixelFormat::Lab8 => Some(Precision::U8),
        PixelFormat::Rgba16 | PixelFormat::Rgb16 | PixelFormat::Gray16 | PixelFormat::GrayA16
        | PixelFormat::Cmyk16 | PixelFormat::CmykA16 | PixelFormat::Lab16 => Some(Precision::U16),
        PixelFormat::RgbaF16 | PixelFormat::RgbF16 | PixelFormat::GrayF16 | PixelFormat::GrayAF16
        | PixelFormat::CmykF16 | PixelFormat::CmykAF16 | PixelFormat::YCbCrF16 => Some(Precision::F16),
        PixelFormat::RgbaF32 | PixelFormat::RgbF32 | PixelFormat::GrayF32 | PixelFormat::GrayAF32
        | PixelFormat::CmykF32 | PixelFormat::CmykAF32 | PixelFormat::YCbCrF32 => Some(Precision::F32),
        _ => None,
    }
}

fn channels(fmt: PixelFormat) -> Option<u32> {
    match fmt {
        PixelFormat::Rgba8 | PixelFormat::Rgba16 | PixelFormat::RgbaF16 | PixelFormat::RgbaF32
        | PixelFormat::Cmyk8 | PixelFormat::Cmyk16 | PixelFormat::CmykF16 | PixelFormat::CmykF32 => Some(0),
        PixelFormat::Rgb8 | PixelFormat::Rgb16 | PixelFormat::RgbF16 | PixelFormat::RgbF32
        | PixelFormat::YCbCr8 | PixelFormat::YCbCrF16 | PixelFormat::YCbCrF32
        | PixelFormat::Lab8 | PixelFormat::Lab16 => Some(1),
        PixelFormat::Gray8 | PixelFormat::Gray16 | PixelFormat::GrayF16 | PixelFormat::GrayF32 => Some(2),
        PixelFormat::GrayA8 | PixelFormat::GrayA16 | PixelFormat::GrayAF16 | PixelFormat::GrayAF32 => Some(3),
        PixelFormat::CmykA8 | PixelFormat::CmykA16 | PixelFormat::CmykAF16 | PixelFormat::CmykAF32 => Some(4),
        _ => None,
    }
}

fn bytes_per_pixel(fmt: PixelFormat) -> Option<u64> {
    Some(fmt.bytes_per_pixel() as u64).filter(|&b| {
        b > 0 && {
            precision(fmt).is_some() && channels(fmt).is_some()
        }
    })
}

fn tf_u32(tf: pixors_engine::common::color::transfer::TransferFn) -> u32 {
    use pixors_engine::common::color::transfer::TransferFn::*;
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
    tile: &pixors_engine::data::tile::Tile,
    src_fmt: PixelFormat,
    dst_fmt: PixelFormat,
    src_cs: ColorSpace,
    dst_cs: ColorSpace,
    alpha: pixors_engine::common::pixel::AlphaPolicy,
    gpu: &pixors_engine::gpu::context::GpuContext,
) -> Result<pixors_engine::data::tile::Tile, Error> {
    let scheduler = gpu.scheduler();

    let in_gbuf = match &tile.data {
        Buffer::Gpu(g) => g,
        other => {
            tracing::error!(
                "[color_convert gpu] non-GPU buffer: {other:?} fmt={src_fmt:?}→{dst_fmt:?}",
            );
            return Err(Error::internal("gpu_dispatch called with non-GPU buffer"));
        }
    };

    let cw = tile.coord.width;
    let ch = tile.coord.height;
    let bpp = bytes_per_pixel(dst_fmt).ok_or_else(|| Error::internal("unknown dst fmt"))?;
    let out_size = cw as u64 * ch as u64 * bpp;

    let conv = pixors_engine::common::color::conversion::ColorConversion::new(src_cs, dst_cs)?;
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
        src_channels: channels(src_fmt).unwrap_or(4),
        dst_channels: channels(dst_fmt).unwrap_or(4),
    };

    let kernel = pixors_shader::kernel::color::ColorConvertParamsKernel::new(
        params,
        src_fmt,
        dst_fmt,
    );

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
