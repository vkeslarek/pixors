//! PNG image loading and saving.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};

use crate::data::buffer::Buffer;
use crate::data::scanline::ScanLine;
use crate::data::tile::TileCoord;
use crate::error::Error;
use crate::graph::item::Item;
use crate::model::color::primaries::RgbPrimaries;
use crate::model::color::space::ColorSpace;
use crate::model::color::transfer::TransferFn;
use crate::model::image::buffer::{BufferDesc, ImageBuffer};
use crate::model::image::decoder::{ImageDecoder, PageStream};
use crate::model::image::desc::{BlendMode, Dpi, ImageDesc, Orientation, PageInfo, PixelOffset};
use crate::model::image::meta::AlphaMode;
use crate::model::pixel::meta::PixelMeta;
use crate::model::pixel::{AlphaPolicy, PixelFormat};

/// PNG format reader.
pub struct PngFormat;

impl PngFormat {
    /// Build the correct BufferDesc for PNG color type + bit depth.
    pub(crate) fn png_buffer_desc(
        info: &png::Info,
        w: u32,
        h: u32,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        is_16bit: bool,
    ) -> BufferDesc {
        match (info.color_type, is_16bit) {
            (png::ColorType::Grayscale, false) => {
                BufferDesc::gray8_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::Grayscale, true) => {
                BufferDesc::gray16be_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::GrayscaleAlpha, false) => {
                BufferDesc::gray_alpha8_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::GrayscaleAlpha, true) => {
                BufferDesc::gray_alpha16be_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::Rgb, false) => {
                BufferDesc::rgb8_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::Rgb, true) => {
                BufferDesc::rgb16be_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::Rgba, false) => {
                BufferDesc::rgba8_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::Rgba, true) => {
                BufferDesc::rgba16be_interleaved(w, h, color_space, alpha_mode)
            }
            (png::ColorType::Indexed, _) => {
                if info.trns.is_some() {
                    BufferDesc::rgba8_interleaved(w, h, color_space, alpha_mode)
                } else {
                    BufferDesc::rgb8_interleaved(w, h, color_space, alpha_mode)
                }
            }
        }
    }

    /// Detects the color space from PNG metadata.
    pub(crate) fn detect_color_space(info: &png::Info) -> ColorSpace {
        use crate::model::color::detect;

        // Priority 1: cICP chunk (new, explicit)
        if let Some(cicp) = info.coding_independent_code_points {
            let primaries = match cicp.color_primaries {
                1 => Some(RgbPrimaries::Bt709),
                9 => Some(RgbPrimaries::Bt2020),
                11 => Some(RgbPrimaries::P3),
                _ => None,
            };
            let transfer = match cicp.transfer_function {
                1 => Some(TransferFn::Rec709Gamma),
                13 => Some(TransferFn::SrgbGamma),
                14 => Some(TransferFn::Gamma22),
                15 => Some(TransferFn::Gamma24),
                16 => Some(TransferFn::ProPhotoGamma),
                _ => None,
            };
            if primaries.is_some() && transfer.is_some() {
                return ColorSpace::with_optional_params(primaries, None, transfer);
            }
        }

        // Priority 2: iCCP chunk
        if let Some(icc_bytes) = &info.icc_profile {
            let classified = detect::IccClassification::classify_icc_profile(icc_bytes);
            if let Some(cs) = classified.color_space {
                return cs;
            }
            tracing::warn!(
                "Unrecognized ICC profile (desc: {}), assuming sRGB",
                String::from_utf8_lossy(&classified.raw)
                    .chars()
                    .take(60)
                    .collect::<String>()
            );
            return ColorSpace::SRGB;
        }

        // Priority 3: sRGB chunk
        if info.srgb.is_some() {
            return ColorSpace::SRGB;
        }

        // Priority 4: gAMA + cHRM chunks (use shared chromaticity matcher)
        let mut gamma = None;
        if let Some(g) = info.gamma() {
            gamma = Some(g.into_value());
        }
        if let Some(chrm) = info.chromaticities() {
            if let Some((prim, wp)) = detect::match_chromaticities(
                chrm.white.0.into_value(),
                chrm.white.1.into_value(),
                chrm.red.0.into_value(),
                chrm.red.1.into_value(),
                chrm.green.0.into_value(),
                chrm.green.1.into_value(),
                chrm.blue.0.into_value(),
                chrm.blue.1.into_value(),
                0.002,
            ) {
                let transfer = gamma
                    .and_then(TransferFn::from_gamma)
                    .unwrap_or(TransferFn::SrgbGamma);
                return ColorSpace::new(prim, wp, transfer);
            }
            if let Some(g) = gamma
                && let Some(tf) = TransferFn::from_gamma(g)
            {
                return ColorSpace::with_optional_params(None, None, Some(tf));
            }
        }

        // Priority 5: gAMA alone
        if let Some(g) = gamma
            && let Some(tf) = TransferFn::from_gamma(g)
        {
            return ColorSpace::with_optional_params(None, None, Some(tf));
        }

        // No color info → assume sRGB
        tracing::warn!("No color space metadata in PNG, assuming sRGB");
        ColorSpace::SRGB
    }
}

// ---------------------------------------------------------------------------
// PngDecoder — implements the new ImageDecoder trait
// ---------------------------------------------------------------------------

pub struct PngDecoder;

impl ImageDecoder for PngDecoder {
    fn probe(&self, path: &Path) -> Result<bool, Error> {
        Ok(path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false))
    }

    fn decode(&self, path: &Path) -> Result<ImageDesc, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader);
        decoder.set_transformations(Transformations::EXPAND);
        let reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();

        let color_space = PngFormat::detect_color_space(info);
        let is_16bit = matches!(info.bit_depth, BitDepth::Sixteen);
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

        let mut exif_tags = HashMap::new();
        for t in &info.uncompressed_latin1_text {
            exif_tags.insert(t.keyword.clone(), t.text.clone());
        }
        for t in &info.compressed_latin1_text {
            exif_tags.insert(t.keyword.clone(), t.get_text().unwrap_or_default());
        }
        for t in &info.utf8_text {
            exif_tags.insert(t.keyword.clone(), t.get_text().unwrap_or_default());
        }

        let icc_profile = info.icc_profile.clone().map(|c| c.into_owned());

        let bit_depth = match info.bit_depth {
            BitDepth::One => 1,
            BitDepth::Two => 2,
            BitDepth::Four => 4,
            BitDepth::Eight => 8,
            BitDepth::Sixteen => 16,
        };

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PNG")
            .to_string();

        let buffer_desc = PngFormat::png_buffer_desc(
            info,
            info.width,
            info.height,
            color_space,
            AlphaMode::Straight,
            is_16bit,
        );

        Ok(ImageDesc {
            format: "PNG".to_string(),
            width: info.width,
            height: info.height,
            bit_depth,
            color_space,
            dpi,
            exif_tags,
            icc_profile,
            pages: vec![PageInfo {
                name,
                buffer_desc,
                offset: PixelOffset::default(),
                opacity: 1.0,
                blend_mode: BlendMode::default(),
                visible: true,
                orientation: Orientation::default(),
            }],
        })
    }

    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error> {
        if page != 0 {
            return Err(Error::invalid_param(format!(
                "PNG has only 1 page, requested {}",
                page
            )));
        }
        let file = File::open(path).map_err(Error::Io)?;
        let mut decoder = Decoder::new(BufReader::new(file));
        decoder.set_transformations(Transformations::EXPAND);
        let reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
        let info = reader.info();

        let color_space = PngFormat::detect_color_space(info);
        let is_16bit = matches!(info.bit_depth, BitDepth::Sixteen);
        let pixel_format = png_pixel_format(info, is_16bit);
        let buffer_desc = PngFormat::png_buffer_desc(
            info,
            info.width,
            info.height,
            color_space,
            AlphaMode::Straight,
            is_16bit,
        );
        let width = info.width;
        let height = info.height;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PNG")
            .to_string();

        Ok(Box::new(PngPageStream {
            reader,
            page_info: PageInfo {
                name,
                buffer_desc,
                offset: PixelOffset::default(),
                opacity: 1.0,
                blend_mode: BlendMode::default(),
                visible: true,
                orientation: Orientation::default(),
            },
            pixel_format,
            color_space,
            is_16bit,
            row: 0,
            height,
            width,
            done: false,
        }))
    }
}

// ---------------------------------------------------------------------------
// PngPageStream — streaming row-by-row decoder
// ---------------------------------------------------------------------------

pub struct PngPageStream {
    reader: png::Reader<BufReader<File>>,
    page_info: PageInfo,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    is_16bit: bool,
    row: u32,
    height: u32,
    width: u32,
    done: bool,
}

impl PageStream for PngPageStream {
    fn page_info(&self) -> &PageInfo {
        &self.page_info
    }

    fn drain(&mut self, max_items: usize) -> Result<Vec<Item>, Error> {
        if self.done {
            return Ok(vec![]);
        }
        let remaining = self.height.saturating_sub(self.row) as usize;
        let count = max_items.min(remaining);
        let mut items = Vec::with_capacity(count);
        let meta = PixelMeta::new(self.pixel_format, self.color_space, AlphaPolicy::Straight);
        let w = self.width;

        for _ in 0..count {
            match self
                .reader
                .next_row()
                .map_err(|e| Error::Png(e.to_string()))?
            {
                Some(row_data) => {
                    let mut raw = row_data.data().to_vec();
                    if self.is_16bit {
                        for chunk in raw.chunks_exact_mut(2) {
                            chunk.swap(0, 1);
                        }
                    }
                    items.push(Item::ScanLine(ScanLine::new(
                        0,
                        self.row,
                        w,
                        meta,
                        Buffer::cpu(raw),
                    )));
                    self.row += 1;
                }
                None => {
                    self.done = true;
                    break;
                }
            }
        }
        if self.row >= self.height {
            self.done = true;
        }
        Ok(items)
    }
}

fn png_pixel_format(info: &png::Info, is_16bit: bool) -> PixelFormat {
    match (info.color_type, is_16bit) {
        (png::ColorType::Grayscale, false) => PixelFormat::Gray8,
        (png::ColorType::Grayscale, true) => PixelFormat::Gray16,
        (png::ColorType::GrayscaleAlpha, false) => PixelFormat::GrayA8,
        (png::ColorType::GrayscaleAlpha, true) => PixelFormat::GrayA16,
        (png::ColorType::Rgb, false) => PixelFormat::Rgb8,
        (png::ColorType::Rgb, true) => PixelFormat::Rgb16,
        (png::ColorType::Rgba, false) => PixelFormat::Rgba8,
        (png::ColorType::Rgba, true) => PixelFormat::Rgba16,
        (png::ColorType::Indexed, _) => {
            if info.trns.is_some() {
                PixelFormat::Rgba8
            } else {
                PixelFormat::Rgb8
            }
        }
    }
}

/// Saves a raw image as PNG to a file path.
pub fn save_png(raw: &ImageBuffer, path: &std::path::Path) -> Result<(), Error> {
    let num_planes = raw.desc.planes.len();

    let (color_type, bit_depth) = match num_planes {
        1 => (ColorType::Grayscale, BitDepth::Eight),
        2 => (ColorType::GrayscaleAlpha, BitDepth::Eight),
        3 => (ColorType::Rgb, BitDepth::Eight),
        4 => (ColorType::Rgba, BitDepth::Eight),
        _ => {
            return Err(Error::unsupported_sample_type(format!(
                "Unsupported number of planes for PNG: {}",
                num_planes
            )));
        }
    };

    let file = File::create(path).map_err(Error::Io)?;
    let w = std::io::BufWriter::new(file);

    let mut encoder = Encoder::new(w, raw.desc.width, raw.desc.height);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);

    match raw.desc.color_space {
        ColorSpace::SRGB => {
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        }
        _ => {
            tracing::warn!(
                "PNG save for {:?} missing metadata; only sRGB is fully supported",
                raw.desc.color_space
            );
        }
    }

    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::Png(e.to_string()))?;
    writer
        .write_image_data(&raw.data)
        .map_err(|e| Error::Png(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn roundtrip_save_load() {
        // TODO: Update test to use new ImageBuffer API
    }
}
