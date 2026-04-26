//! TIFF image loading.
//!
//! Supports 8-bit and 16-bit RGB, RGBA, and grayscale images.
//! Color space detection via PhotometricInterpretation tag.

use crate::color::ColorSpace;
use crate::error::Error;
use crate::image::buffer::BufferDesc;
use crate::image::{AlphaMode, ImageBuffer};
use crate::io::ImageReader;
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

    fn read_metadata(&self, path: &Path) -> Result<(u32, u32, ColorSpace, AlphaMode), Error> {
        read_tiff_metadata(path)
    }

    fn load(&self, path: &Path) -> Result<ImageBuffer, Error> {
        load_tiff(path)
    }
}

/// Read TIFF metadata: dimensions, color space, and alpha mode.
pub fn read_tiff_metadata(path: &Path) -> Result<(u32, u32, ColorSpace, AlphaMode), Error> {
    let file = File::open(path).map_err(Error::Io)?;
    let reader = BufReader::new(file);
    let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;

    let (width, height) = decoder.dimensions().map_err(|e| Error::Tiff(e.to_string()))?;
    let color_space = detect_tiff_color_space(&mut decoder);
    let alpha_mode = AlphaMode::Straight;

    Ok((width, height, color_space, alpha_mode))
}

/// Loads a TIFF image from a file path into an ImageBuffer.
pub fn load_tiff(path: &Path) -> Result<ImageBuffer, Error> {
    let _sw = crate::debug_stopwatch!("load_tiff");
    tracing::info!("Reading TIFF from {:?}", path);

    let file = File::open(path).map_err(Error::Io)?;
    let reader = BufReader::new(file);
    let mut decoder = Decoder::new(reader).map_err(|e| Error::Tiff(e.to_string()))?;

    let (width, height) = decoder.dimensions().map_err(|e| Error::Tiff(e.to_string()))?;
    let color_type = decoder.colortype().map_err(|e| Error::Tiff(e.to_string()))?;
    let color_space = detect_tiff_color_space(&mut decoder);

    let alpha_mode = match color_type {
        tiff::ColorType::RGBA(..) | tiff::ColorType::GrayA(..) => AlphaMode::Straight,
        _ => AlphaMode::Opaque,
    };

    tracing::debug!(
        "TIFF: {}x{} color_type={:?}",
        width,
        height,
        color_type
    );

    let image = match decoder.read_image().map_err(|e| Error::Tiff(e.to_string()))? {
        DecodingResult::U8(data) => {
            let desc = match color_type {
                tiff::ColorType::RGB(8) => {
                    BufferDesc::rgb8_interleaved(width, height, color_space, alpha_mode)
                }
                tiff::ColorType::RGBA(8) => {
                    BufferDesc::rgba8_interleaved(width, height, color_space, alpha_mode)
                }
                tiff::ColorType::Gray(8) => {
                    BufferDesc::gray8_interleaved(width, height, color_space, alpha_mode)
                }
                tiff::ColorType::GrayA(8) => {
                    BufferDesc::gray_alpha8_interleaved(width, height, color_space, alpha_mode)
                }
                _ => {
                    return Err(Error::unsupported_sample_type(format!(
                        "Unsupported TIFF 8-bit color type: {:?}",
                        color_type
                    )));
                }
            };
            ImageBuffer { desc, data }
        }
        DecodingResult::U16(data) => {
            // Convert native u16 → native-endian bytes (matches SampleFormat::U16Le on LE hosts)
            let bytes: Vec<u8> = data
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();

            let desc = match color_type {
                tiff::ColorType::RGB(16) => {
                    BufferDesc::rgb16_interleaved(width, height, color_space, alpha_mode)
                }
                tiff::ColorType::RGBA(16) => {
                    BufferDesc::rgba16_interleaved(width, height, color_space, alpha_mode)
                }
                tiff::ColorType::Gray(16) => {
                    BufferDesc::gray16_interleaved(width, height, color_space, alpha_mode)
                }
                _ => {
                    return Err(Error::unsupported_sample_type(format!(
                        "Unsupported TIFF 16-bit color type: {:?}",
                        color_type
                    )));
                }
            };
            ImageBuffer { desc, data: bytes }
        }
        other => {
            return Err(Error::unsupported_sample_type(format!(
                "Unsupported TIFF sample format: {:?}",
                other
            )));
        }
    };

    Ok(image)
}

/// Detects the color space from TIFF metadata.
fn detect_tiff_color_space(decoder: &mut Decoder<BufReader<File>>) -> ColorSpace {
    // Priority 1: PhotometricInterpretation (tag 262)
    if let Ok(photometric) = decoder.find_tag_unsigned::<u32>(Tag::PhotometricInterpretation) {
        match photometric {
            Some(2) => return ColorSpace::SRGB, // RGB
            Some(1) => {
                // BlackIsZero — grayscale, assume sRGB transfer
                return ColorSpace::SRGB;
            }
            _ => {}
        }
    }

    // Fallback
    tracing::warn!("No color space metadata in TIFF, assuming sRGB");
    ColorSpace::SRGB
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_tiff_metadata_nonexistent() {
        let result = read_tiff_metadata(Path::new("/nonexistent/file.tiff"));
        assert!(result.is_err());
    }
}
