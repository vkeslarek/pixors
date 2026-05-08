pub mod encoder;
mod stream;
mod tags;

pub use encoder::TiffEncoder;
pub use stream::{TiffPageStream, tiff_pixel_format, tiff_row_bytes};
pub use tags::{
    count_tiff_pages, detect_tiff_color_space, read_color_map, read_exif_blob, read_extra_samples,
    read_icc_profile, read_orientation, read_page_name, read_page_offset, read_planar_config,
    read_rational_tag, read_tag_ascii, read_white_is_zero, read_ycbcr_subsampling,
};

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ::exif as exif_crate;
use ::tiff;

use crate::common::pixel::AlphaPolicy;
use crate::error::Error;

use super::codec::{ImageDecoder, PageStream};
use super::*;

pub struct TiffDecoder;

impl ImageDecoder for TiffDecoder {
    fn probe(&self, path: &Path) -> Result<bool, Error> {
        Ok(path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("tiff") || e.eq_ignore_ascii_case("tif"))
            .unwrap_or(false))
    }

    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder =
            tiff::decoder::Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = decoder
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder
            .colortype()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let bit_depth = ct.bit_depth();
        let icc_profile = read_icc_profile(&mut decoder);
        let color_space = detect_tiff_color_space(&mut decoder, icc_profile.as_deref());

        let dpi = {
            let xres = read_rational_tag(&mut decoder, tiff::tags::Tag::XResolution);
            let yres = read_rational_tag(&mut decoder, tiff::tags::Tag::YResolution);
            match (xres, yres) {
                (Some(x), Some(y)) => {
                    let unit = decoder
                        .find_tag_unsigned::<u32>(tiff::tags::Tag::ResolutionUnit)
                        .ok()
                        .flatten()
                        .unwrap_or(2);
                    let scale = if unit == 3 { 2.54 } else { 1.0 };
                    Some(Dpi {
                        x: x * scale,
                        y: y * scale,
                    })
                }
                _ => None,
            }
        };

        let mut metadata = Vec::new();
        metadata.push(Metadata::ImageWidth(w));
        metadata.push(Metadata::ImageHeight(h));
        let photometric = decoder
            .find_tag_unsigned::<u32>(tiff::tags::Tag::PhotometricInterpretation)
            .ok()
            .flatten();
        if let Some(pm) = photometric {
            metadata.push(Metadata::PhotometricInterpretation(pm as u16));
        }

        if let Some(ref dpi_val) = dpi {
            metadata.push(Metadata::Dpi {
                x: dpi_val.x,
                y: dpi_val.y,
            });
        }

        if let Some(ref icc) = icc_profile {
            metadata.push(Metadata::IccProfile(icc.clone()));
        }

        // ── Standard TIFF tags → Metadata ──────────────────────────────
        macro_rules! push_tag {
            ($decoder:expr, $tag:expr, $variant:ident) => {
                if let Some(v) = read_tag_ascii($decoder, $tag) {
                    metadata.push(Metadata::$variant(v));
                }
            };
        }
        push_tag!(&mut decoder, tiff::tags::Tag::Make, Make);
        push_tag!(&mut decoder, tiff::tags::Tag::Model, Model);
        push_tag!(&mut decoder, tiff::tags::Tag::Software, Software);
        push_tag!(&mut decoder, tiff::tags::Tag::DateTime, DateTime);
        push_tag!(&mut decoder, tiff::tags::Tag::Artist, Artist);
        push_tag!(&mut decoder, tiff::tags::Tag::Copyright, Copyright);
        push_tag!(&mut decoder, tiff::tags::Tag::ImageDescription, Description);

        // ── EXIF extraction ────────────────────────────────────────────
        if let Some(exif_bytes) = read_exif_blob(&mut decoder)
            && let Ok((exif_fields, _le)) = exif_crate::parse_exif(&exif_bytes)
        {
            metadata.extend(super::exif::from_exif_fields(&exif_fields));
        }

        let first_orientation = read_orientation(&mut decoder);
        metadata.push(Metadata::Orientation(first_orientation as u16));

        let page_count = count_tiff_pages(&mut decoder);
        let mut pages = Vec::with_capacity(page_count);

        for i in 0..page_count {
            decoder
                .seek_to_image(i)
                .map_err(|e| Error::Tiff(e.to_string()))?;
            let (pw, ph) = decoder
                .dimensions()
                .map_err(|e| Error::Tiff(e.to_string()))?;
            let pct = decoder
                .colortype()
                .map_err(|e| Error::Tiff(e.to_string()))?;
            let pcs = detect_tiff_color_space(&mut decoder, icc_profile.as_deref());
            let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", i + 1));
            let (ox, oy) = read_page_offset(&mut decoder, pw, ph);
            let orientation = read_orientation(&mut decoder);
            let extra = read_extra_samples(&mut decoder);

            let has_alpha = matches!(
                pct,
                tiff::ColorType::RGBA(..) | tiff::ColorType::GrayA(..) | tiff::ColorType::CMYKA(..)
            ) || extra.is_some();
            let alpha_policy = match extra {
                Some(1) => AlphaPolicy::PremultiplyOnPack,
                _ if has_alpha => AlphaPolicy::Straight,
                _ => AlphaPolicy::OpaqueDrop,
            };
            pages.push(PageInfo {
                name,
                color_space: pcs,
                alpha_policy,
                offset: PixelOffset { x: ox, y: oy },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
                delay_ms: 0,
                dispose: DisposeOp::None,
            });
        }

        Ok(ImageDescriptor {
            format: "TIFF".to_string(),
            width: w,
            height: h,
            bit_depth,
            color_space,
            dpi,
            metadata,
            icc_profile,
            pages,
        })
    }

    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder =
            tiff::decoder::Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;
        decoder
            .seek_to_image(page)
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = decoder
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder
            .colortype()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let cs = detect_tiff_color_space(&mut decoder, None);
        let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", page + 1));
        let (ox, oy) = read_page_offset(&mut decoder, w, h);
        let orientation = read_orientation(&mut decoder);
        let extra = read_extra_samples(&mut decoder);
        let planar = read_planar_config(&mut decoder);
        let white_is_zero = read_white_is_zero(&mut decoder);
        let photometric = decoder
            .find_tag_unsigned::<u32>(tiff::tags::Tag::PhotometricInterpretation)
            .ok()
            .flatten();
        let palette = if matches!(ct, tiff::ColorType::Palette(_)) {
            read_color_map(&mut decoder)
        } else {
            None
        };
        let _subsampling = read_ycbcr_subsampling(&mut decoder);

        let image_data = decoder
            .read_image()
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let pixel_format = tiff_pixel_format(&image_data, ct, photometric);
        let has_alpha = matches!(
            ct,
            tiff::ColorType::RGBA(..) | tiff::ColorType::GrayA(..) | tiff::ColorType::CMYKA(..)
        ) || extra.is_some();
        let alpha_policy = match extra {
            Some(1) => AlphaPolicy::PremultiplyOnPack,
            _ if has_alpha => AlphaPolicy::Straight,
            _ => AlphaPolicy::OpaqueDrop,
        };

        Ok(Box::new(TiffPageStream::new(
            PageInfo {
                name,
                color_space: cs,
                alpha_policy,
                offset: PixelOffset { x: ox, y: oy },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
                delay_ms: 0,
                dispose: DisposeOp::None,
            },
            image_data,
            ct,
            pixel_format,
            w,
            h,
            planar,
            white_is_zero,
            palette,
        )))
    }
}
