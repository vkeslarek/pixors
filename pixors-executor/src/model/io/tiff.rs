//! TIFF image loading.

use crate::model::color::space::ColorSpace;
use crate::error::Error;
use crate::model::image::buffer::BufferDesc;
use crate::model::image::{AlphaMode, ImageBuffer, Image, Layer, LayerMetadata, ImageInfo, Orientation, ImageMetadata, BlendMode};
use crate::model::io::ImageReader;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;

/// TIFF format reader.
pub struct TiffFormat;

impl ImageReader for TiffFormat {
    fn can_handle(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("tiff") || e.eq_ignore_ascii_case("tif"))
            .unwrap_or(false)
    }

    fn read_document_info(&self, path: &Path) -> Result<ImageInfo, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;
        let metadata = read_tiff_document_metadata(&mut decoder, path);
        let layer_count = count_tiff_pages(&mut decoder);
        Ok(ImageInfo { layer_count, metadata })
    }

    /// Single-pass decode: open once, walk IFDs sequentially.
    fn load_document(&self, path: &Path) -> Result<Image, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;
        let metadata = read_tiff_document_metadata(&mut decoder, path);
        let mut layers = Vec::new();
        let mut idx = 0;

        loop {
            let (w, h) = decoder.dimensions().map_err(|e| Error::Tiff(e.to_string()))?;
            let ct = decoder.colortype().map_err(|e| Error::Tiff(e.to_string()))?;
            let cs = detect_tiff_color_space(&mut decoder);
            let am = alpha_mode_for(ct);
            let image = match decoder.read_image().map_err(|e| Error::Tiff(e.to_string()))? {
                DecodingResult::U8(data) => decode_u8_tiff(w, h, ct, cs, am, data),
                DecodingResult::U16(data) => decode_u16_tiff(w, h, ct, cs, am, data),
                DecodingResult::U32(data) => decode_u32_tiff(w, h, ct, cs, am, data),
                DecodingResult::F32(data) => decode_f32_tiff(w, h, ct, cs, am, data),
                other => return Err(Error::unsupported_sample_type(format!("Unsupported: {:?}", other))),
            }?;
            let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", idx + 1));
            let offset = read_page_offset(&mut decoder, w, h);
            let orientation = read_orientation(&mut decoder);
            layers.push(Layer {
                name,
                buffer: image,
                offset,
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
            });

            if !decoder.more_images() { break; }
            decoder.next_image().map_err(|e| Error::Tiff(e.to_string()))?;
            idx += 1;
        }

        Ok(Image { layers, metadata })
    }

    fn read_layer_metadata(&self, path: &Path, layer: usize) -> Result<LayerMetadata, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;

        // Step to the requested IFD
        for _ in 0..layer {
            if !decoder.more_images() {
                return Err(Error::invalid_param(format!("TIFF layer {} out of bounds", layer)));
            }
            decoder.next_image().map_err(|e| Error::Tiff(e.to_string()))?;
        }

        let (w, h) = decoder.dimensions().map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder.colortype().map_err(|e| Error::Tiff(e.to_string()))?;
        let cs = detect_tiff_color_space(&mut decoder);
        let am = alpha_mode_for(ct);
        let desc = tiff_buffer_desc(ct, w, h, cs, am)?;
        let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", layer + 1));

        Ok(LayerMetadata {
            desc,
            orientation: Orientation::Identity,
            offset: (0, 0),
            name,
        })
    }

    fn load_layer(&self, path: &Path, layer: usize) -> Result<Layer, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;

        // Step to the requested IFD
        for _ in 0..layer {
            if !decoder.more_images() {
                return Err(Error::invalid_param(format!("TIFF layer {} out of bounds", layer)));
            }
            decoder.next_image().map_err(|e| Error::Tiff(e.to_string()))?;
        }

        let (w, h) = decoder.dimensions().map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder.colortype().map_err(|e| Error::Tiff(e.to_string()))?;
        let cs = detect_tiff_color_space(&mut decoder);
        let am = alpha_mode_for(ct);
        let image = match decoder.read_image().map_err(|e| Error::Tiff(e.to_string()))? {
            DecodingResult::U8(data) => decode_u8_tiff(w, h, ct, cs, am, data),
            DecodingResult::U16(data) => decode_u16_tiff(w, h, ct, cs, am, data),
            DecodingResult::U32(data) => decode_u32_tiff(w, h, ct, cs, am, data),
            DecodingResult::F32(data) => decode_f32_tiff(w, h, ct, cs, am, data),
            other => Err(Error::unsupported_sample_type(format!("Unsupported TIFF: {:?}", other))),
        }?;
        let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", layer + 1));
        let offset = read_page_offset(&mut decoder, w, h);
        let orientation = read_orientation(&mut decoder);
        Ok(Layer {
            name,
            buffer: image,
            offset,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            orientation,
        })
    }
}

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
    w: u32, h: u32,
    cs: ColorSpace,
    am: AlphaMode,
) -> Result<BufferDesc, Error> {
    Ok(match ct {
        tiff::ColorType::Gray(8)    => BufferDesc::gray8_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(8)   => BufferDesc::gray_alpha8_interleaved(w, h, cs, am),
        tiff::ColorType::RGB(8)     => BufferDesc::rgb8_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(8)    => BufferDesc::rgba8_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(16)   => BufferDesc::gray16_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(16)  => BufferDesc::gray_alpha16_interleaved(w, h, cs, am),
        tiff::ColorType::RGB(16)    => BufferDesc::rgb16_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(16)   => BufferDesc::rgba16_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(32)   => BufferDesc::gray32_interleaved(w, h, cs, am),
        tiff::ColorType::RGB(32)    => BufferDesc::rgb32_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(32)   => BufferDesc::rgba32_interleaved(w, h, cs, am),
        _ => return Err(Error::unsupported_sample_type(format!("BufferDesc for {:?}", ct))),
    })
}

/// Read the TIFF PageName tag (285) if present.
fn read_page_name(decoder: &mut Decoder<BufReader<File>>) -> Option<String> {
    decoder.find_tag_unsigned::<u32>(Tag::Unknown(285)).ok().flatten()
        .map(|_| String::from("(page name tag)"))
}

/// Read page offset from XPosition/YPosition tags (286/287).
fn read_page_offset(decoder: &mut Decoder<BufReader<File>>, _w: u32, _h: u32) -> (i32, i32) {
    let x = decoder.find_tag_unsigned::<u32>(Tag::Unknown(286)).ok().flatten();
    let y = decoder.find_tag_unsigned::<u32>(Tag::Unknown(287)).ok().flatten();
    match (x, y) {
        (Some(xv), Some(yv)) => (xv as i32, yv as i32),
        _ => (0, 0),
    }
}

/// Read Orientation tag (274) — 1..8 EXIF-style.
fn read_orientation(decoder: &mut Decoder<BufReader<File>>) -> Orientation {
    let raw = decoder.find_tag_unsigned::<u32>(Tag::Unknown(274)).ok().flatten();
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
    w: u32, h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<u8>,
) -> Result<ImageBuffer, Error> {
    let desc = match ct {
        tiff::ColorType::RGB(8)    => BufferDesc::rgb8_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(8)   => BufferDesc::rgba8_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(8)   => BufferDesc::gray8_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(8)  => BufferDesc::gray_alpha8_interleaved(w, h, cs, am),
        tiff::ColorType::YCbCr(8)  => return Err(Error::unsupported_sample_type("YCbCr decode not implemented")),
        tiff::ColorType::CMYK(8)   => return Err(Error::unsupported_sample_type("CMYK without ICC not supported — convert to RGB first")),
        tiff::ColorType::CMYKA(8)  => return Err(Error::unsupported_sample_type("CMYK without ICC not supported — convert to RGB first")),
        tiff::ColorType::Lab(8)    => return Err(Error::unsupported_sample_type("Lab decode not implemented")),
        _ => return Err(Error::unsupported_sample_type(format!("Unsupported 8-bit color: {:?}", ct))),
    };
    Ok(ImageBuffer { desc, data })
}

fn decode_u16_tiff(
    w: u32, h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<u16>,
) -> Result<ImageBuffer, Error> {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = match ct {
        tiff::ColorType::RGB(16)   => BufferDesc::rgb16_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(16)  => BufferDesc::rgba16_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(16)  => BufferDesc::gray16_interleaved(w, h, cs, am),
        tiff::ColorType::GrayA(16) => BufferDesc::gray_alpha16_interleaved(w, h, cs, am),
        tiff::ColorType::CMYK(16)  => return Err(Error::unsupported_sample_type("CMYK without ICC not supported — convert to RGB first")),
        tiff::ColorType::CMYKA(16) => return Err(Error::unsupported_sample_type("CMYK without ICC not supported — convert to RGB first")),
        tiff::ColorType::Lab(16)   => return Err(Error::unsupported_sample_type("Lab decode not implemented")),
        _ => return Err(Error::unsupported_sample_type(format!("Unsupported 16-bit color: {:?}", ct))),
    };
    Ok(ImageBuffer { desc, data: bytes })
}

fn decode_u32_tiff(
    w: u32, h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<u32>,
) -> Result<ImageBuffer, Error> {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = match ct {
        tiff::ColorType::RGB(32)   => BufferDesc::rgb32_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(32)  => BufferDesc::rgba32_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(32)  => BufferDesc::gray32_interleaved(w, h, cs, am),
        _ => return Err(Error::unsupported_sample_type(format!("Unsupported 32-bit color: {:?}", ct))),
    };
    Ok(ImageBuffer { desc, data: bytes })
}

fn decode_f32_tiff(
    w: u32, h: u32,
    ct: tiff::ColorType,
    cs: ColorSpace,
    am: AlphaMode,
    data: Vec<f32>,
) -> Result<ImageBuffer, Error> {
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = match ct {
        tiff::ColorType::RGB(32)   => BufferDesc::rgb_f32_interleaved(w, h, cs, am),
        tiff::ColorType::RGBA(32)  => BufferDesc::rgba_f32_interleaved(w, h, cs, am),
        tiff::ColorType::Gray(32)  => BufferDesc::gray_f32_interleaved(w, h, cs, am),
        _ => return Err(Error::unsupported_sample_type(format!("Unsupported F32 color: {:?}", ct))),
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
pub fn read_tiff_document_metadata(
    decoder: &mut Decoder<BufReader<File>>,
    path: &Path,
) -> ImageMetadata {
    use std::collections::HashMap;

    let source_format = Some("TIFF".to_string());
    let source_path = Some(path.to_path_buf());
    let text = HashMap::new();

    // DPI from XResolution/YResolution (tags 282/283) and ResolutionUnit (296)
    let dpi = {
        let (Ok(Some(xres)), Ok(Some(yres))) = (
            decoder.find_tag_unsigned::<u32>(Tag::XResolution),
            decoder.find_tag_unsigned::<u32>(Tag::YResolution),
        ) else { return ImageMetadata { source_format, source_path, dpi: None, text, raw_icc: None } };
        let unit = decoder.find_tag_unsigned::<u32>(Tag::ResolutionUnit).ok().flatten().unwrap_or(2);
        let scale = if unit == 3 { 2.54 } else { 1.0 };
        Some((xres as f32 * scale, yres as f32 * scale))
    };

    // ICC profile (tag 34675)
    let raw_icc = read_tag_bytes(decoder, Tag::Unknown(34675));

    ImageMetadata { source_format, source_path, dpi, text, raw_icc }
}

#[allow(dead_code)]
fn read_tag_string(decoder: &mut Decoder<BufReader<File>>, tag: Tag) -> Option<String> {
    decoder.find_tag_unsigned::<u32>(tag).ok().flatten().map(|_| {
        format!("(see raw tag {})", tag.to_u16())
    })
}

fn read_tag_bytes(decoder: &mut Decoder<BufReader<File>>, tag: Tag) -> Option<Vec<u8>> {
    decoder.find_tag_unsigned::<u32>(tag).ok().flatten().map(|_| Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_tiff_metadata_nonexistent() {
        let result = TiffFormat.read_document_info(Path::new("/nonexistent/file.tiff"));
        assert!(result.is_err());
    }
}
