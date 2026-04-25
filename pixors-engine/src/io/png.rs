//! PNG image loading and saving.

use crate::color::{ColorSpace, TransferFn, RgbPrimaries, WhitePoint};
use crate::error::Error;
use crate::image::buffer::BufferDesc;
use crate::image::{AlphaMode, ImageBuffer};
use crate::io::ImageReader;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use png::{Decoder, Encoder, ColorType, BitDepth, Transformations};

/// PNG format reader.
pub struct PngFormat;

impl ImageReader for PngFormat {
    fn can_handle(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            .unwrap_or(false)
    }

    fn read_metadata(&self, path: &Path) -> Result<(u32, u32, ColorSpace, AlphaMode), Error> {
        read_png_metadata(path)
    }

    fn load(&self, path: &Path) -> Result<ImageBuffer, Error> {
        load_png(path)
    }

    fn stream_to_tiles_sync(
        &self,
        path: &Path,
        width: u32,
        height: u32,
        tile_size: u32,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        store: &crate::storage::TileStore,
        on_progress: Option<&(dyn Fn(u8) + Send)>,
    ) -> Result<(), Error> {
        stream_png_to_tiles_sync(path, width, height, tile_size, color_space, alpha_mode, store, on_progress)
    }
}

/// Read PNG metadata: dimensions, color space, and alpha mode.
pub fn read_png_metadata(path: &Path) -> Result<(u32, u32, ColorSpace, AlphaMode), Error> {
    let file = File::open(path).map_err(Error::Io)?;
    let reader = BufReader::new(file);
    let mut decoder = Decoder::new(reader);
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);

    let reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;
    let info = reader.info();

    let width = info.width;
    let height = info.height;
    let color_space = detect_color_space(info);
    let alpha_mode = AlphaMode::Straight;

    Ok((width, height, color_space, alpha_mode))
}

/// Load a PNG into an ImageBuffer, then stream tiles to TileStore.
pub fn stream_png_to_tiles_sync(
    path: &Path,
    width: u32,
    height: u32,
    tile_size: u32,
    _color_space: ColorSpace,
    _alpha_mode: AlphaMode,
    store: &crate::storage::TileStore,
    on_progress: Option<&(dyn Fn(u8) + Send)>,
) -> Result<(), Error> {
    let image = load_png(path)?;
    assert_eq!(image.desc.width, width);
    assert_eq!(image.desc.height, height);
    crate::io::stream_image_buffer_to_tiles(&image, tile_size, store, on_progress)
}

/// Loads a PNG image from a file path.
pub fn load_png(path: &Path) -> Result<ImageBuffer, Error> {
    let _sw = crate::debug_stopwatch!("load_png");
    tracing::info!("Reading PNG from {:?}", path);

    let file = File::open(path).map_err(Error::Io)?;
    let reader = BufReader::new(file);

    let mut decoder = Decoder::new(reader);
    // Apply transformations to expand palette, grayscale, etc.
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);

    let mut reader = decoder.read_info().map_err(|e| Error::Png(e.to_string()))?;

    let info = reader.info();
    let width = info.width;
    let height = info.height;

    // Determine color space from PNG chunks
    let color_space = detect_color_space(&info);

    // Alpha mode: PNG stores straight alpha
    let alpha_mode = AlphaMode::Straight;

    // Create appropriate BufferDesc based on color type
    // PNG transformations guarantee RGBA or RGB output (8-bit)
    let desc = match info.color_type {
        png::ColorType::Grayscale => {
            BufferDesc::gray8_interleaved(width, height, color_space, alpha_mode)
        }
        png::ColorType::GrayscaleAlpha => {
            BufferDesc::gray_alpha8_interleaved(width, height, color_space, alpha_mode)
        }
        png::ColorType::Rgb => {
            BufferDesc::rgb8_interleaved(width, height, color_space, alpha_mode)
        }
        png::ColorType::Rgba => {
            BufferDesc::rgba8_interleaved(width, height, color_space, alpha_mode)
        }
        png::ColorType::Indexed => {
            // Expanded to RGB/RGBA by Transformations::EXPAND
            if info.trns.is_some() {
                BufferDesc::rgba8_interleaved(width, height, color_space, alpha_mode)
            } else {
                BufferDesc::rgb8_interleaved(width, height, color_space, alpha_mode)
            }
        }
    };

    // Read raw pixel data
    let buf_size = reader.output_buffer_size().expect("PNG decoder output buffer size unavailable");
    let mut buf = vec![0; buf_size];
    let info = reader.next_frame(&mut buf).map_err(|e| Error::Png(e.to_string()))?;
    let data = buf[..info.buffer_size()].to_vec();

    Ok(ImageBuffer { desc, data })
}

/// Detects the color space from PNG metadata.
fn detect_color_space(info: &png::Info) -> ColorSpace {
    // Priority 1: cICP chunk (new, explicit)
    if let Some(cicp) = info.coding_independent_code_points {
        let primaries = match cicp.color_primaries {
            1  => Some(RgbPrimaries::Bt709),
            9  => Some(RgbPrimaries::Bt2020),
            11 => Some(RgbPrimaries::P3),
            _  => None,
        };
        let transfer = match cicp.transfer_function {
            1  => Some(TransferFn::Rec709Gamma),
            13 => Some(TransferFn::SrgbGamma),
            14 => Some(TransferFn::Gamma22),
            15 => Some(TransferFn::Gamma24),
            16 => Some(TransferFn::ProPhotoGamma),
            _  => None,
        };
        if primaries.is_some() && transfer.is_some() {
            return crate::color::color_space_from_params(primaries, None, transfer);
        }
    }

    // Priority 2: iCCP chunk
    if let Some(icc_profile) = &info.icc_profile {
        if icc_profile.len() >= 20 && &icc_profile[0..4] == b"sRGB" {
            return ColorSpace::SRGB;
        }
        tracing::warn!("Unrecognized ICC profile, assuming sRGB");
        return ColorSpace::SRGB;
    }
    
    // Priority 3: sRGB chunk
    if info.srgb.is_some() {
        return ColorSpace::SRGB;
    }
    
    // Priority 4: gAMA + cHRM chunks
    let mut gamma = None;
    if let Some(g) = info.gamma() {
        gamma = Some(g.into_value());
    }
    if let Some(chrm) = info.chromaticities() {
        let known = [
            (RgbPrimaries::Bt709,     (0.3127, 0.3290, 0.64,   0.33,  0.30,  0.60,  0.15,  0.06)),
            (RgbPrimaries::Adobe1998, (0.3127, 0.3290, 0.64,   0.33,  0.21,  0.71,  0.15,  0.06)),
            (RgbPrimaries::P3,        (0.3127, 0.3290, 0.680,  0.320, 0.265, 0.690, 0.150, 0.060)),
        ];
        for (prim, (wx, wy, rx, ry, gx, gy, bx, by)) in known.iter() {
            if (chrm.white.0.into_value() - wx).abs() < 0.001 &&
               (chrm.white.1.into_value() - wy).abs() < 0.001 &&
               (chrm.red.0.into_value() - rx).abs() < 0.001 &&
               (chrm.red.1.into_value() - ry).abs() < 0.001 &&
               (chrm.green.0.into_value() - gx).abs() < 0.001 &&
               (chrm.green.1.into_value() - gy).abs() < 0.001 &&
               (chrm.blue.0.into_value() - bx).abs() < 0.001 &&
               (chrm.blue.1.into_value() - by).abs() < 0.001 {
                let transfer = gamma
                    .and_then(|g| crate::color::transfer_from_gamma(g))
                    .unwrap_or(TransferFn::SrgbGamma);
                return ColorSpace::new(*prim, WhitePoint::D65, transfer);
            }
        }
        if let Some(g) = gamma {
            return crate::color::color_space_from_params(None, None, crate::color::transfer_from_gamma(g));
        }
    }
    
    // Priority 5: gAMA alone
    if let Some(g) = gamma {
        return crate::color::color_space_from_params(None, None, crate::color::transfer_from_gamma(g));
    }
    
    // No color info → assume sRGB
    tracing::warn!("No color space metadata in PNG, assuming sRGB");
    ColorSpace::SRGB
}

/// Saves a raw image as PNG to a file path.
pub fn save_png(raw: &ImageBuffer, path: &std::path::Path) -> Result<(), Error> {
    let num_planes = raw.desc.planes.len();

    let (color_type, bit_depth) = match num_planes {
        1 => (ColorType::Grayscale, BitDepth::Eight),
        2 => (ColorType::GrayscaleAlpha, BitDepth::Eight),
        3 => (ColorType::Rgb, BitDepth::Eight),
        4 => (ColorType::Rgba, BitDepth::Eight),
        _ => return Err(Error::unsupported_sample_type(
            format!("Unsupported number of planes for PNG: {}", num_planes)
        )),
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

    let mut writer = encoder.write_header().map_err(|e| Error::Png(e.to_string()))?;
    writer.write_image_data(&raw.data).map_err(|e| Error::Png(e.to_string()))?;
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
