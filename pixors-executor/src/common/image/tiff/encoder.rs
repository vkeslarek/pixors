use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::common::image::codec::{
    EncoderConfig, EncoderDescriptor, ImageEncoder, TiffBitDepth, TiffLayout, TiffPredictor,
    TiffVariant,
};
use crate::error::Error;

pub struct TiffEncoder;

impl ImageEncoder for TiffEncoder {
    fn probe(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("tiff") || e.eq_ignore_ascii_case("tif"))
            .unwrap_or(false)
    }

    fn encode(
        &self,
        path: &Path,
        data: &[u8],
        desc: &EncoderDescriptor,
        config: &EncoderConfig,
    ) -> Result<(), Error> {
        let tiff_cfg = match config {
            EncoderConfig::Tiff(cfg) => cfg,
            _ => return Err(Error::internal("wrong config type for TiffEncoder")),
        };

        let file = File::create(path).map_err(Error::Io)?;
        let writer = BufWriter::new(file);

        match tiff_cfg.tiff_variant {
            TiffVariant::Classic => {
                let mut enc = tiff::encoder::TiffEncoder::new(writer)
                    .map_err(|e| Error::Tiff(e.to_string()))?;
                enc = enc.with_compression(tiff_compression(&tiff_cfg.compression));
                if let Some(p) = tiff_predictor(&tiff_cfg.compression) {
                    enc = enc.with_predictor(p);
                }
                encode_pixels(&mut enc, data, desc, tiff_cfg)
            }
            TiffVariant::BigTiff => {
                let mut enc = tiff::encoder::TiffEncoder::new_big(writer)
                    .map_err(|e| Error::Tiff(e.to_string()))?;
                enc = enc.with_compression(tiff_compression(&tiff_cfg.compression));
                if let Some(p) = tiff_predictor(&tiff_cfg.compression) {
                    enc = enc.with_predictor(p);
                }
                encode_pixels(&mut enc, data, desc, tiff_cfg)
            }
        }
    }
}

fn deflate_level(l: u8) -> tiff::encoder::DeflateLevel {
    if l <= 1 {
        tiff::encoder::DeflateLevel::Fast
    } else if l <= 6 {
        tiff::encoder::DeflateLevel::Balanced
    } else {
        tiff::encoder::DeflateLevel::Best
    }
}

fn tiff_compression(c: &crate::common::image::codec::TiffCompression) -> tiff::encoder::Compression {
    match c {
        crate::common::image::codec::TiffCompression::None => tiff::encoder::Compression::Uncompressed,
        crate::common::image::codec::TiffCompression::PackBits => tiff::encoder::Compression::Packbits,
        crate::common::image::codec::TiffCompression::Lzw { .. } => tiff::encoder::Compression::Lzw,
        crate::common::image::codec::TiffCompression::Deflate { level, .. } => {
            tiff::encoder::Compression::Deflate(deflate_level(*level))
        }
    }
}

fn tiff_predictor(
    c: &crate::common::image::codec::TiffCompression,
) -> Option<tiff::encoder::Predictor> {
    use crate::common::image::codec::TiffCompression;
    let pred = match c {
        TiffCompression::Lzw { predictor } | TiffCompression::Deflate { predictor, .. } => *predictor,
        _ => TiffPredictor::None,
    };
    match pred {
        TiffPredictor::None => Some(tiff::encoder::Predictor::None),
        TiffPredictor::Horizontal => Some(tiff::encoder::Predictor::Horizontal),
        TiffPredictor::FloatingPoint => None,
    }
}

fn encode_pixels<W: std::io::Write + std::io::Seek, K: tiff::encoder::TiffKind>(
    encoder: &mut tiff::encoder::TiffEncoder<W, K>,
    data: &[u8],
    desc: &EncoderDescriptor,
    cfg: &crate::common::image::codec::TiffExportConfig,
) -> Result<(), Error> {
    use crate::common::pixel::PixelFormat;
    use tiff::encoder::colortype;
    use tiff::tags::ExtraSamples;

    match (desc.pixel_format, cfg.bit_depth) {
        (PixelFormat::Gray8, TiffBitDepth::Eight) => {
            encode::<_, colortype::Gray8, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::Gray16, TiffBitDepth::Sixteen) => {
            encode::<_, colortype::Gray16, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::GrayF32, TiffBitDepth::ThirtyTwo) => {
            encode::<_, colortype::Gray32Float, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::GrayA8, TiffBitDepth::Eight) => encode::<_, colortype::Gray8, _>(
            encoder,
            data,
            desc,
            cfg,
            Some(&[ExtraSamples::UnassociatedAlpha]),
        ),
        (PixelFormat::GrayA16, TiffBitDepth::Sixteen) => encode::<_, colortype::Gray16, _>(
            encoder,
            data,
            desc,
            cfg,
            Some(&[ExtraSamples::UnassociatedAlpha]),
        ),
        (PixelFormat::Rgb8, TiffBitDepth::Eight) => {
            encode::<_, colortype::RGB8, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::Rgb16, TiffBitDepth::Sixteen) => {
            encode::<_, colortype::RGB16, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::RgbF32, TiffBitDepth::ThirtyTwo) => {
            encode::<_, colortype::RGB32Float, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::Rgba8, TiffBitDepth::Eight) => {
            encode::<_, colortype::RGBA8, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::Rgba16, TiffBitDepth::Sixteen) => {
            encode::<_, colortype::RGBA16, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::RgbaF32, TiffBitDepth::ThirtyTwo) => {
            encode::<_, colortype::RGBA32Float, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::Cmyk8, TiffBitDepth::Eight) => {
            encode::<_, colortype::CMYK8, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::Cmyk16, TiffBitDepth::Sixteen) => {
            encode::<_, colortype::CMYK16, _>(encoder, data, desc, cfg, None)
        }
        (PixelFormat::CmykA8, TiffBitDepth::Eight) => {
            encode::<_, colortype::CMYKA8, _>(encoder, data, desc, cfg, None)
        }
        _ => Err(Error::invalid_param(format!(
            "unsupported TIFF pixel/bit-depth combination: {:?} @ {:?}",
            desc.pixel_format, cfg.bit_depth
        ))),
    }
}

fn encode<
    W: std::io::Write + std::io::Seek,
    C: tiff::encoder::colortype::ColorType,
    K: tiff::encoder::TiffKind,
>(
    encoder: &mut tiff::encoder::TiffEncoder<W, K>,
    data: &[u8],
    desc: &EncoderDescriptor,
    cfg: &crate::common::image::codec::TiffExportConfig,
    extra: Option<&[tiff::tags::ExtraSamples]>,
) -> Result<(), Error>
where
    [C::Inner]: tiff::encoder::TiffValue,
    C::Inner: bytemuck::Pod,
{
    use tiff::tags::{ResolutionUnit, Tag};

    let mut image = encoder
        .new_image::<C>(desc.width, desc.height)
        .map_err(|e| Error::Tiff(e.to_string()))?;

    if let Some(extra_samples) = extra {
        image
            .extra_samples(extra_samples)
            .map_err(|e| Error::Tiff(e.to_string()))?;
    }

    if cfg.embed_dpi
        && let Some(ref dpi) = desc.dpi
    {
        let xres = tiff::encoder::Rational {
            n: (dpi.x * 10000.0) as u32,
            d: 10000,
        };
        let yres = tiff::encoder::Rational {
            n: (dpi.y * 10000.0) as u32,
            d: 10000,
        };
        image
            .encoder()
            .write_tag(Tag::XResolution, xres)
            .map_err(|e: tiff::TiffError| Error::Tiff(e.to_string()))?;
        image
            .encoder()
            .write_tag(Tag::YResolution, yres)
            .map_err(|e: tiff::TiffError| Error::Tiff(e.to_string()))?;
        image
            .encoder()
            .write_tag(Tag::ResolutionUnit, ResolutionUnit::Inch.to_u16())
            .map_err(|e: tiff::TiffError| Error::Tiff(e.to_string()))?;
    }

    if cfg.embed_icc
        && let Some(ref icc) = desc.icc_profile
    {
        image
            .encoder()
            .write_tag(Tag::IccProfile, icc.as_slice())
            .map_err(|e: tiff::TiffError| Error::Tiff(e.to_string()))?;
    }

    let orientation_val = match cfg.orientation {
        crate::common::image::Orientation::Identity => 1u16,
        crate::common::image::Orientation::FlipH => 2,
        crate::common::image::Orientation::Rotate180 => 3,
        crate::common::image::Orientation::FlipV => 4,
        crate::common::image::Orientation::Transpose => 5,
        crate::common::image::Orientation::Rotate90 => 6,
        crate::common::image::Orientation::Transverse => 7,
        crate::common::image::Orientation::Rotate270 => 8,
    };
    image
        .encoder()
        .write_tag(Tag::Orientation, orientation_val)
        .map_err(|e| Error::Tiff(e.to_string()))?;

    write_meta_tags(image.encoder(), &cfg.tags)?;

    if let TiffLayout::Strip { rows_per_strip } = cfg.layout {
        image
            .rows_per_strip(rows_per_strip)
            .map_err(|e| Error::Tiff(e.to_string()))?;
    }

    image
        .write_data(bytemuck::cast_slice(data))
        .map_err(|e| Error::Tiff(e.to_string()))?;

    Ok(())
}

fn write_meta_tags<W: std::io::Write + std::io::Seek, K: tiff::encoder::TiffKind>(
    dir: &mut tiff::encoder::DirectoryEncoder<W, K>,
    tags: &crate::common::image::codec::TiffMetaTags,
) -> Result<(), Error> {
    use tiff::tags::Tag;
    macro_rules! opt {
        ($tag:expr, $field:expr) => {
            if let Some(ref val) = $field {
                dir.write_tag($tag, val.as_str())
                    .map_err(|e| Error::Tiff(e.to_string()))?;
            }
        };
    }
    opt!(Tag::Make, tags.make);
    opt!(Tag::Model, tags.model);
    opt!(Tag::Software, tags.software);
    opt!(Tag::DateTime, tags.date_time);
    opt!(Tag::Artist, tags.artist);
    opt!(Tag::Copyright, tags.copyright);
    opt!(Tag::ImageDescription, tags.image_description);
    Ok(())
}
