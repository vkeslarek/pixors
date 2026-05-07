mod color;
mod stream;

pub use color::detect_color_space;
pub use stream::{png_pixel_format, PngPageStream};

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ::png as png;
use ::exif as exif_crate;

use crate::error::Error;
use crate::common::pixel::AlphaPolicy;

use super::codec::{ImageDecoder, PageStream};
use super::*;

pub struct PngDecoder;

impl ImageDecoder for PngDecoder {
    fn probe(&self, path: &Path) -> Result<bool, Error> {
        Ok(path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false))
    }

    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = png::Decoder::new(reader);
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;

        let color_space = detect_color_space(reader.info());
        let animated = reader.info().is_animated();

        // Collect APNG frame metadata while we have mutable reader access
        let mut apng_frames = Vec::new();
        if animated {
            let mut frame_num = 1u32;
            while let Ok(frame) = reader.next_frame_info() {
                        let delay = (frame.delay_num as u32 * 1000)
                            .div_ceil(frame.delay_den.max(1) as u32);
                        let dispose = match frame.dispose_op {
                            png::DisposeOp::None => DisposeOp::None,
                            png::DisposeOp::Background => DisposeOp::Background,
                            png::DisposeOp::Previous => DisposeOp::Previous,
                        };
                        let blend = match frame.blend_op {
                            png::BlendOp::Source => BlendMode::Source,
                            png::BlendOp::Over => BlendMode::Over,
                        };
                        apng_frames.push((delay, dispose, blend, frame.x_offset, frame.y_offset, frame_num));
                        frame_num += 1;
            }
        }
        let info = reader.info();

        let dpi = info.pixel_dims.and_then(|pdim| {
            if pdim.unit == png::Unit::Meter {
                Some(Dpi {
                    x: pdim.xppu as f32 * 0.0254,
                    y: pdim.yppu as f32 * 0.0254,
                })
            } else {
                None
            }
        });

        let mut metadata = Vec::new();
        metadata.push(Metadata::ImageWidth(info.width));
        metadata.push(Metadata::ImageHeight(info.height));

        if let Some(dpi_val) = dpi {
            metadata.push(Metadata::Dpi { x: dpi_val.x, y: dpi_val.y });
        }

        let mut text_pairs: Vec<(&str, String)> = Vec::new();
        for t in &info.uncompressed_latin1_text {
            text_pairs.push((t.keyword.as_str(), t.text.clone()));
        }
        for t in &info.compressed_latin1_text {
            text_pairs.push((t.keyword.as_str(), t.get_text().unwrap_or_default()));
        }
        for t in &info.utf8_text {
            text_pairs.push((t.keyword.as_str(), t.get_text().unwrap_or_default()));
        }
        metadata.extend(super::exif::from_png_text(&text_pairs));

        if let Some(ref exif_bytes) = info.exif_metadata
            && let Ok((exif_fields, _little_endian)) = exif_crate::parse_exif(exif_bytes)
        {
            metadata.extend(super::exif::from_exif_fields(&exif_fields));
        }

        let icc_profile = info.icc_profile.clone().map(|c| c.into_owned());

        if let Some(ref icc) = icc_profile {
            metadata.push(Metadata::IccProfile(icc.clone()));
        }

        let bit_depth = match info.bit_depth {
            png::BitDepth::One => 1,
            png::BitDepth::Two => 2,
            png::BitDepth::Four => 4,
            png::BitDepth::Eight => 8,
            png::BitDepth::Sixteen => 16,
        };

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PNG")
            .to_string();

        let mut pages = vec![PageInfo {
            name: name.clone(),
            color_space,
            alpha_policy: AlphaPolicy::Straight,
            offset: PixelOffset::default(),
            opacity: 1.0,
            blend_mode: BlendMode::default(),
            visible: true,
            orientation: Orientation::default(),
            delay_ms: 0,
            dispose: DisposeOp::None,
        }];

        // ── APNG frames ────────────────────────────────────────────
        for (delay, dispose, blend, x_off, y_off, frame_num) in apng_frames {
            pages.push(PageInfo {
                name: format!("{name} frame {frame_num}"),
                color_space,
                alpha_policy: AlphaPolicy::Straight,
                offset: PixelOffset { x: x_off as i32, y: y_off as i32 },
                opacity: 1.0,
                blend_mode: blend,
                visible: true,
                orientation: Orientation::default(),
                delay_ms: delay,
                dispose,
            });
        }

        Ok(ImageDescriptor {
            format: "PNG".to_string(),
            width: info.width,
            height: info.height,
            bit_depth,
            color_space,
            dpi,
            metadata,
            icc_profile,
            pages,
        })
    }

    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error> {
        if page != 0 {
            return Err(Error::invalid_param(format!(
                "PNG page {page} — APNG frame reading not yet implemented"
            )));
        }
        let file = File::open(path).map_err(Error::Io)?;
        let mut decoder = png::Decoder::new(BufReader::new(file));
        decoder.set_transformations(png::Transformations::EXPAND);
        let reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();

        let color_space = detect_color_space(info);
        let is_16bit = matches!(info.bit_depth, png::BitDepth::Sixteen);
        let pixel_format = png_pixel_format(info, is_16bit);
        let width = info.width;
        let height = info.height;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PNG")
            .to_string();

        Ok(Box::new(PngPageStream::new(
            reader,
            PageInfo {
                name,
                color_space,
                alpha_policy: AlphaPolicy::Straight,
                offset: PixelOffset::default(),
                opacity: 1.0,
                blend_mode: BlendMode::default(),
                visible: true,
                orientation: Orientation::default(),
                delay_ms: 0,
                dispose: DisposeOp::None,
            },
            pixel_format,
            color_space,
            is_16bit,
            width,
            height,
        )))
    }
}
