//! TIFF image loading.

use crate::error::Error;
use crate::model::color::space::ColorSpace;
use crate::data::buffer::Buffer;
use crate::data::scanline::ScanLine;
use crate::graph::item::Item;
use crate::model::image::buffer::{BufferDescriptor, ImageBuffer};
use crate::model::image::desc::{BlendMode, Dpi, ImageDescriptor, Orientation, PageInfo, PixelOffset};
use crate::model::io::{ImageDecoder, PageStream};
use crate::model::image::meta::AlphaMode;
use crate::model::pixel::PixelFormat;
use crate::model::pixel::meta::PixelMeta;
use crate::model::pixel::AlphaPolicy;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;

/// TIFF format reader.

/// Count pages in a TIFF by iterating IFDs.
fn count_tiff_pages(decoder: &mut Decoder<BufReader<File>>) -> usize {
    let mut count = 1;
    while decoder.more_images() {
        if decoder.next_image().is_ok() {
            count += 1;
        } else {
            break;
        }
    }
    count
}

/// Build a BufferDesc from TIFF color type info.
fn tiff_buffer_desc(
    ct: tiff::ColorType,
    w: u32,
    h: u32,
    cs: ColorSpace,
    am: AlphaMode,
) -> Result<BufferDescriptor, Error> {
    Ok(match ct {
        tiff::ColorType::Gray(8) => BufferDescriptor::gray8_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(8) => BufferDescriptor::gray_alpha8_interleaved(w, h, cs, am),
        tiff::ColorType::RGB(8) => BufferDescriptor::rgb8_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(8) => BufferDescriptor::rgba8_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(16) => BufferDescriptor::gray16_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(16) => BufferDescriptor::gray_alpha16_interleaved(w, h, cs, am),
        tiff::ColorType::RGB(16) => BufferDescriptor::rgb16_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(16) => BufferDescriptor::rgba16_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(32) => BufferDescriptor::gray32_interleaved(w, h, cs, am),
        tiff::ColorType::RGB(32) => BufferDescriptor::rgb32_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(32) => BufferDescriptor::rgba32_interleaved(w, h, cs, am),
        _ => {
            return Err(Error::unsupported_sample_type(format!(
                "BufferDesc for {:?}",
                ct
            )));
        }
    })
}

/// Read the TIFF PageName tag (285) if present.
fn read_page_name(decoder: &mut Decoder<BufReader<File>>) -> Option<String> {
    decoder
        .find_tag_unsigned::<u32>(Tag::Unknown(285))
        .ok()
        .flatten()
        .map(|_| String::from("(page name tag)"))
}

/// Read page offset from XPosition/YPosition tags (286/287).
fn read_page_offset(decoder: &mut Decoder<BufReader<File>>, _w: u32, _h: u32) -> (i32, i32) {
    let x = decoder
        .find_tag_unsigned::<u32>(Tag::Unknown(286))
        .ok()
        .flatten();
    let y = decoder
        .find_tag_unsigned::<u32>(Tag::Unknown(287))
        .ok()
        .flatten();
    match (x, y) {
        (Some(xv), Some(yv)) => (xv as i32, yv as i32),
        _ => (0, 0),
    }
}

/// Read Orientation tag (274) — 1..8 EXIF-style.
fn read_orientation(decoder: &mut Decoder<BufReader<File>>) -> Orientation {
    let raw = decoder
        .find_tag_unsigned::<u32>(Tag::Unknown(274))
        .ok()
        .flatten();
    match raw {
        Some(2) => Orientation::FlipH,
        Some(3) => Orientation::Rotate180,
        Some(4) => Orientation::FlipV,
        Some(5) => Orientation::Transpose,
        Some(6) => Orientation::Rotate90,
        Some(7) => Orientation::Transverse,
        Some(8) => Orientation::Rotate270,
        _ => Orientation::Identity,
    }
}

// ---------------------------------------------------------------------------
// Existing decode helpers (unchanged)
// ---------------------------------------------------------------------------

fn decode_u8_tiff(
    w: u32,
    h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<u8>,
) -> Result<ImageBuffer, Error> {
    let desc = match ct {
        tiff::ColorType::RGB(8) => BufferDescriptor::rgb8_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(8) => BufferDescriptor::rgba8_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(8) => BufferDescriptor::gray8_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(8) => BufferDescriptor::gray_alpha8_interleaved(w, h, cs, am),
        tiff::ColorType::YCbCr(8) => {
            return Err(Error::unsupported_sample_type(
                "YCbCr decode not implemented",
            ));
        }
        tiff::ColorType::CMYK(8) => {
            return Err(Error::unsupported_sample_type(
                "CMYK without ICC not supported — convert to RGB first",
            ));
        }
        tiff::ColorType::CMYKA(8) => {
            return Err(Error::unsupported_sample_type(
                "CMYK without ICC not supported — convert to RGB first",
            ));
        }
        tiff::ColorType::Lab(8) => {
            return Err(Error::unsupported_sample_type("Lab decode not implemented"));
        }
        _ => {
            return Err(Error::unsupported_sample_type(format!(
                "Unsupported 8-bit color: {:?}",
                ct
            )));
        }
    };
    Ok(ImageBuffer { desc, data })
}

fn decode_u16_tiff(
    w: u32,
    h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<u16>,
) -> Result<ImageBuffer, Error> {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = match ct {
        tiff::ColorType::RGB(16) => BufferDescriptor::rgb16_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(16) => BufferDescriptor::rgba16_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(16) => BufferDescriptor::gray16_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(16) => BufferDescriptor::gray_alpha16_interleaved(w, h, cs, am),
        tiff::ColorType::CMYK(16) => {
            return Err(Error::unsupported_sample_type(
                "CMYK without ICC not supported — convert to RGB first",
            ));
        }
        tiff::ColorType::CMYKA(16) => {
            return Err(Error::unsupported_sample_type(
                "CMYK without ICC not supported — convert to RGB first",
            ));
        }
        tiff::ColorType::Lab(16) => {
            return Err(Error::unsupported_sample_type("Lab decode not implemented"));
        }
        _ => {
            return Err(Error::unsupported_sample_type(format!(
                "Unsupported 16-bit color: {:?}",
                ct
            )));
        }
    };
    Ok(ImageBuffer { desc, data: bytes })
}

fn decode_u32_tiff(
    w: u32,
    h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<u32>,
) -> Result<ImageBuffer, Error> {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = match ct {
        tiff::ColorType::RGB(32) => BufferDescriptor::rgb32_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(32) => BufferDescriptor::rgba32_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(32) => BufferDescriptor::gray32_interleaved(w, h, cs, am),
        _ => {
            return Err(Error::unsupported_sample_type(format!(
                "Unsupported 32-bit color: {:?}",
                ct
            )));
        }
    };
    Ok(ImageBuffer { desc, data: bytes })
}

fn decode_f32_tiff(
    w: u32,
    h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<f32>,
) -> Result<ImageBuffer, Error> {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = match ct {
        tiff::ColorType::RGB(32) => BufferDescriptor::rgb_f32_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(32) => BufferDescriptor::rgba_f32_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(32) => BufferDescriptor::gray_f32_interleaved(w, h, cs, am),
        _ => {
            return Err(Error::unsupported_sample_type(format!(
                "Unsupported F32 color: {:?}",
                ct
            )));
        }
    };
    Ok(ImageBuffer { desc, data: bytes })
}

/// Map TIFF color type to alpha mode.
fn alpha_mode_for(ct: tiff::ColorType) -> AlphaMode {
    match ct {
        tiff::ColorType::RGBA(..) | tiff::ColorType::GrayA(..) => AlphaMode::Straight,
        _ => AlphaMode::Opaque,
    }
}

// ---------------------------------------------------------------------------
// Color space detection
// ---------------------------------------------------------------------------

fn detect_tiff_color_space(decoder: &mut Decoder<BufReader<File>>) -> ColorSpace {
    // PhotometricInterpretation (tag 262)
    if let Ok(photometric) = decoder.find_tag_unsigned::<u32>(Tag::PhotometricInterpretation) {
        match photometric {
            Some(2) => return ColorSpace::SRGB, // RGB — assume sRGB as baseline
            Some(1) => return ColorSpace::SRGB, // BlackIsZero grayscale → sRGB transfer
            _ => {}
        }
    }

    // Fallback
    tracing::warn!("No color space metadata in TIFF, assuming sRGB");
    ColorSpace::SRGB
}

/// Extract document-level metadata from a TIFF decoder.
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
        let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = decoder
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder
            .colortype()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let bit_depth = ct.bit_depth();
        let color_space = detect_tiff_color_space(&mut decoder);

        let dpi = {
            let xres = decoder.find_tag_unsigned::<u32>(Tag::XResolution).ok().flatten();
            let yres = decoder.find_tag_unsigned::<u32>(Tag::YResolution).ok().flatten();
            match (xres, yres) {
                (Some(x), Some(y)) => {
                    let unit = decoder.find_tag_unsigned::<u32>(Tag::ResolutionUnit).ok().flatten().unwrap_or(2);
                    let scale = if unit == 3 { 2.54 } else { 1.0 };
                    Some(Dpi { x: x as f32 * scale, y: y as f32 * scale })
                }
                _ => None,
            }
        };
        let icc_profile = decoder.find_tag_unsigned::<u32>(Tag::Unknown(34675))
            .ok()
            .flatten()
            .map(|_| Vec::new());

        let mut exif_tags = std::collections::HashMap::new();
        let first_orientation = read_orientation(&mut decoder);
        exif_tags.insert(
            "Orientation".to_string(),
            format!("{:?}", first_orientation),
        );

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
            let pcs = detect_tiff_color_space(&mut decoder);
            let pam = alpha_mode_for(pct);
            let buffer_desc = tiff_buffer_desc(pct, pw, ph, pcs, pam)?;
            let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", i + 1));
            let (ox, oy) = read_page_offset(&mut decoder, pw, ph);
            let orientation = read_orientation(&mut decoder);

            pages.push(PageInfo {
                name,
                buffer_desc,
                offset: PixelOffset { x: ox, y: oy },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
            });
        }

        Ok(ImageDescriptor {
            format: "TIFF".to_string(),
            width: w,
            height: h,
            bit_depth,
            color_space,
            dpi,
            exif_tags,
            icc_profile,
            pages,
        })
    }

    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;
        decoder
            .seek_to_image(page)
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = decoder
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder
            .colortype()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let cs = detect_tiff_color_space(&mut decoder);
        let am = alpha_mode_for(ct);
        let buffer_desc = tiff_buffer_desc(ct, w, h, cs, am)?;
        let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", page + 1));
        let (ox, oy) = read_page_offset(&mut decoder, w, h);
        let orientation = read_orientation(&mut decoder);

        let image_data = decoder
            .read_image()
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let pixel_format = tiff_pixel_format(&image_data, ct);

        Ok(Box::new(TiffPageStream {
            page_info: PageInfo {
                name,
                buffer_desc,
                offset: PixelOffset { x: ox, y: oy },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
            },
            image_data,
            color_type: ct,
            pixel_format,
            width: w,
            height: h,
            row: 0,
            done: false,
        }))
    }
}

// ---------------------------------------------------------------------------
// TiffPageStream — streaming row-by-row decoder
// ---------------------------------------------------------------------------

pub struct TiffPageStream {
    page_info: PageInfo,
    image_data: DecodingResult,
    color_type: tiff::ColorType,
    pixel_format: PixelFormat,
    width: u32,
    height: u32,
    row: u32,
    done: bool,
}

impl PageStream for TiffPageStream {
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
        let meta = PixelMeta::new(
            self.pixel_format,
            self.page_info.buffer_desc.color_space,
            AlphaPolicy::Straight,
        );

        for _ in 0..count {
            let raw = tiff_row_bytes(&self.image_data, self.row, self.width, self.color_type)?;
            items.push(Item::ScanLine(ScanLine::new(
                0,
                self.row,
                self.width,
                meta,
                Buffer::cpu(raw),
            )));
            self.row += 1;
        }

        if self.row >= self.height {
            self.done = true;
        }
        Ok(items)
    }
}

// ---------------------------------------------------------------------------
// Row-to-RGBA8 conversion helpers
// ---------------------------------------------------------------------------

fn tiff_pixel_format(result: &DecodingResult, ct: tiff::ColorType) -> PixelFormat {
    match result {
        DecodingResult::U8(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::Gray8,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayA8,
            tiff::ColorType::RGB(_) => PixelFormat::Rgb8,
            tiff::ColorType::RGBA(_) => PixelFormat::Rgba8,
            _ => {
                tracing::warn!("Unsupported U8 TIFF color: {:?}, falling back to Rgba8", ct);
                PixelFormat::Rgba8
            }
        },
        DecodingResult::U16(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::Gray16,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayA16,
            tiff::ColorType::RGB(_) => PixelFormat::Rgb16,
            tiff::ColorType::RGBA(_) => PixelFormat::Rgba16,
            _ => {
                tracing::warn!(
                    "Unsupported U16 TIFF color: {:?}, falling back to Rgba16",
                    ct
                );
                PixelFormat::Rgba16
            }
        },
        DecodingResult::F32(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF32,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF32,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF32,
            _ => {
                tracing::warn!(
                    "Unsupported F32 TIFF color: {:?}, falling back to RgbaF32",
                    ct
                );
                PixelFormat::RgbaF32
            }
        },
        DecodingResult::U32(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF32,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF32,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF32,
            _ => {
                tracing::warn!(
                    "Unsupported U32 TIFF color: {:?}, falling back to RgbaF32",
                    ct
                );
                PixelFormat::RgbaF32
            }
        },
        _ => {
            tracing::warn!("Unknown TIFF DecodingResult, falling back to Rgba8");
            PixelFormat::Rgba8
        }
    }
}

fn tiff_row_bytes(
    result: &DecodingResult,
    row: u32,
    width: u32,
    ct: tiff::ColorType,
) -> Result<Vec<u8>, Error> {
    let w = width as usize;
    let spp = ct.num_samples() as usize;
    let row_start = row as usize * w * spp;
    let row_end = row_start + w * spp;

    match result {
        DecodingResult::U8(data) => Ok(data[row_start..row_end].to_vec()),
        DecodingResult::U16(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        DecodingResult::U32(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        DecodingResult::F32(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        _ => Err(Error::unsupported_sample_type(format!(
            "Unsupported TIFF sample type for row decode: {:?}",
            result
        ))),
    }
}

#[cfg(test)]
mod tests {}
