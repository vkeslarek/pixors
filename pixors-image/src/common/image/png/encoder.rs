use std::borrow::Cow;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::common::image::codec::{
    EncoderConfig, EncoderDescriptor, ImageEncoder, PngBitDepth, PngColorType, PngCompression,
    PngFilter, PngTextEncoding,
};
use pixors_engine::error::Error;

pub struct PngEncoder;

impl ImageEncoder for PngEncoder {
    fn probe(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false)
    }

    fn encode(
        &self,
        path: &Path,
        data: &[u8],
        desc: &EncoderDescriptor,
        config: &EncoderConfig,
    ) -> Result<(), Error> {
        let png_cfg = match config {
            EncoderConfig::Png(cfg) => cfg,
            _ => return Err(Error::internal("wrong config type for PngEncoder")),
        };
        write_png(path, data, desc, png_cfg)
    }
}

fn write_png(
    path: &Path,
    data: &[u8],
    desc: &EncoderDescriptor,
    cfg: &crate::common::image::codec::PngExportConfig,
) -> Result<(), Error> {
    let file = File::create(path).map_err(Error::Io)?;
    let w = BufWriter::new(file);

    // Build Info to carry ICC profile (no direct setter on Encoder).
    let mut info = png::Info::with_size(desc.width, desc.height);
    if cfg.embed_icc
        && let Some(ref icc) = desc.icc_profile
    {
        info.icc_profile = Some(Cow::Owned(icc.clone()));
    }

    let mut encoder = png::Encoder::with_info(w, info).map_err(|e| Error::Png(e.to_string()))?;

    encoder.set_color(png_color_type(cfg.color_type));
    encoder.set_depth(png_bit_depth(cfg.bit_depth));
    encoder.set_compression(png_compression(cfg.compression));
    encoder.set_deflate_compression(deflate_compression(cfg.compression));
    encoder.set_filter(png_filter(cfg.filter));

    if let Some(srgb) = cfg.srgb_intent {
        encoder.set_source_srgb(png_srgb_intent(srgb));
    }

    if let Some(gamma) = cfg.gamma {
        encoder.set_source_gamma(png::ScaledFloat::new(gamma as f32));
    }

    if cfg.embed_dpi
        && let Some(ref dpi) = desc.dpi
    {
        let xppu = (dpi.x as f64 * 100.0 / 2.54).round() as u32;
        let yppu = (dpi.y as f64 * 100.0 / 2.54).round() as u32;
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu,
            yppu,
            unit: png::Unit::Meter,
        }));
    }

    for chunk in &cfg.text_chunks {
        match chunk.encoding {
            PngTextEncoding::Text => {
                encoder
                    .add_text_chunk(chunk.keyword.clone(), chunk.text.clone())
                    .map_err(|e| Error::Png(e.to_string()))?;
            }
            PngTextEncoding::Ztxt => {
                encoder
                    .add_ztxt_chunk(chunk.keyword.clone(), chunk.text.clone())
                    .map_err(|e| Error::Png(e.to_string()))?;
            }
            PngTextEncoding::Itxt => {
                encoder
                    .add_itxt_chunk(chunk.keyword.clone(), chunk.text.clone())
                    .map_err(|e| Error::Png(e.to_string()))?;
            }
        }
    }

    if let Some(ref anim) = cfg.animation {
        let n_frames = anim.frames.len() as u32;
        encoder
            .set_animated(n_frames, anim.num_plays)
            .map_err(|e| Error::Png(e.to_string()))?;
        for frame in &anim.frames {
            encoder
                .set_frame_delay(frame.delay_numerator, frame.delay_denominator)
                .map_err(|e| Error::Png(e.to_string()))?;
            encoder
                .set_dispose_op(match frame.dispose_op {
                    crate::common::image::codec::PngDisposeOp::None => png::DisposeOp::None,
                    crate::common::image::codec::PngDisposeOp::Background => {
                        png::DisposeOp::Background
                    }
                    crate::common::image::codec::PngDisposeOp::Previous => png::DisposeOp::Previous,
                })
                .map_err(|e| Error::Png(e.to_string()))?;
            encoder
                .set_blend_op(match frame.blend_op {
                    crate::common::image::codec::PngBlendOp::Source => png::BlendOp::Source,
                    crate::common::image::codec::PngBlendOp::Over => png::BlendOp::Over,
                })
                .map_err(|e| Error::Png(e.to_string()))?;
        }
    }

    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::Png(e.to_string()))?;

    writer
        .write_image_data(data)
        .map_err(|e| Error::Png(e.to_string()))?;

    writer.finish().map_err(|e| Error::Png(e.to_string()))?;

    Ok(())
}

fn png_color_type(ct: PngColorType) -> png::ColorType {
    match ct {
        PngColorType::Grayscale => png::ColorType::Grayscale,
        PngColorType::GrayscaleAlpha => png::ColorType::GrayscaleAlpha,
        PngColorType::Rgb => png::ColorType::Rgb,
        PngColorType::Rgba => png::ColorType::Rgba,
    }
}

fn png_bit_depth(bd: PngBitDepth) -> png::BitDepth {
    match bd {
        PngBitDepth::One => png::BitDepth::One,
        PngBitDepth::Two => png::BitDepth::Two,
        PngBitDepth::Four => png::BitDepth::Four,
        PngBitDepth::Eight => png::BitDepth::Eight,
        PngBitDepth::Sixteen => png::BitDepth::Sixteen,
    }
}

fn png_compression(c: PngCompression) -> png::Compression {
    match c {
        PngCompression::None => png::Compression::NoCompression,
        PngCompression::Fast => png::Compression::Fast,
        PngCompression::Default => png::Compression::Balanced,
        PngCompression::Best => png::Compression::High,
        PngCompression::Level(_) => png::Compression::Balanced,
    }
}

fn deflate_compression(c: PngCompression) -> png::DeflateCompression {
    match c {
        PngCompression::None => png::DeflateCompression::NoCompression,
        PngCompression::Fast => png::DeflateCompression::FdeflateUltraFast,
        PngCompression::Default => png::DeflateCompression::default(),
        PngCompression::Best => png::DeflateCompression::Level(9),
        PngCompression::Level(l) => png::DeflateCompression::Level(l),
    }
}

fn png_filter(f: PngFilter) -> png::Filter {
    match f {
        PngFilter::None => png::Filter::NoFilter,
        PngFilter::Sub => png::Filter::Sub,
        PngFilter::Up => png::Filter::Up,
        PngFilter::Average => png::Filter::Avg,
        PngFilter::Paeth => png::Filter::Paeth,
        PngFilter::Adaptive => png::Filter::Adaptive,
    }
}

fn png_srgb_intent(i: crate::common::image::codec::PngSrgbIntent) -> png::SrgbRenderingIntent {
    match i {
        crate::common::image::codec::PngSrgbIntent::Perceptual => {
            png::SrgbRenderingIntent::Perceptual
        }
        crate::common::image::codec::PngSrgbIntent::RelativeColorimetric => {
            png::SrgbRenderingIntent::RelativeColorimetric
        }
        crate::common::image::codec::PngSrgbIntent::Saturation => {
            png::SrgbRenderingIntent::Saturation
        }
        crate::common::image::codec::PngSrgbIntent::AbsoluteColorimetric => {
            png::SrgbRenderingIntent::AbsoluteColorimetric
        }
    }
}
