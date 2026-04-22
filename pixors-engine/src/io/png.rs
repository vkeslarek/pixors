//! PNG image loading and saving.

use crate::error::Error;
use crate::image::{RawImage, AlphaMode, SampleType, ChannelLayoutKind, SampleLayout};
use crate::color::{ColorSpace, TransferFn, RgbPrimaries, WhitePoint};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use png::{Decoder, Encoder, ColorType, BitDepth, Transformations};

/// Loads a PNG image from a file path.
pub fn load_png(path: &Path) -> Result<RawImage, Error> {
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
    
    // Determine sample type
    let sample_type = match info.bit_depth {
        png::BitDepth::Eight => SampleType::U8,
        png::BitDepth::Sixteen => SampleType::U16,
        png::BitDepth::One | png::BitDepth::Two | png::BitDepth::Four => {
            // Expanded to 8-bit by Transformations::EXPAND
            SampleType::U8
        }
    };
    
    // Determine channel layout
    let channel_layout = match info.color_type {
        png::ColorType::Grayscale => ChannelLayoutKind::Gray,
        png::ColorType::GrayscaleAlpha => ChannelLayoutKind::GrayAlpha,
        png::ColorType::Rgb => ChannelLayoutKind::Rgb,
        png::ColorType::Rgba => ChannelLayoutKind::Rgba,
        png::ColorType::Indexed => {
            // Expanded to RGB/RGBA by Transformations::EXPAND
            if info.trns.is_some() {
                ChannelLayoutKind::Rgba
            } else {
                ChannelLayoutKind::Rgb
            }
        }
    };
    
    // PNG is always interleaved
    let sample_layout = SampleLayout::Interleaved;
    
    // Determine color space from PNG chunks
    let color_space = detect_color_space(&info);
    
    // Alpha mode: PNG stores straight alpha
    let alpha_mode = AlphaMode::Straight;
    
    // Read raw pixel data
    let buf_size = reader.output_buffer_size().expect("PNG decoder output buffer size unavailable");
    let mut buf = vec![0; buf_size];
    let info = reader.next_frame(&mut buf).map_err(|e| Error::Png(e.to_string()))?;
    let data = buf[..info.buffer_size()].to_vec();
    
    RawImage::new(
        width,
        height,
        sample_type,
        channel_layout,
        sample_layout,
        color_space,
        alpha_mode,
        data,
    )
}

/// Detects the color space from PNG metadata.
fn detect_color_space(info: &png::Info) -> ColorSpace {
    // Priority 1: cICP chunk (new, explicit)
    if let Some(cicp) = info.coding_independent_code_points {
        // cicp is (color_primaries, transfer_function, matrix_coefficients, video_full_range_flag)
        // We ignore matrix_coefficients and video_full_range_flag for now.
        let primaries = match cicp.color_primaries {
            1 => Some(RgbPrimaries::Bt709),
            9 => Some(RgbPrimaries::Bt2020),
            11 => Some(RgbPrimaries::P3),
            12 => {
                tracing::warn!("cICP color primaries 12 (BT.470 System M) not supported, treating as unknown");
                None
            }
            13 => {
                tracing::warn!("cICP color primaries 13 (BT.470 System B/G) not supported, treating as unknown");
                None
            }
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
        if let (Some(primaries), Some(transfer)) = (primaries, transfer) {
            // Assume D65 white point for standard primaries
            return ColorSpace::new(primaries, WhitePoint::D65, transfer);
        }
    }
    
    // Priority 2: iCCP chunk
    if let Some(icc_profile) = &info.icc_profile {
        // Simplified: detect sRGB ICC profile
        if icc_profile.len() >= 20 && &icc_profile[0..4] == b"sRGB" {
            return ColorSpace::SRGB;
        }
        // TODO: parse ICC profile properly (Phase 2)
        // For now, fall back to sRGB with warning
        tracing::warn!("Unrecognized ICC profile, assuming sRGB");
        return ColorSpace::SRGB;
    }
    
    // Priority 3: sRGB chunk
    if info.srgb.is_some() {
        // srgb_intent is rendering intent, ignore for color space
        return ColorSpace::SRGB;
    }
    
    // Priority 4: gAMA + cHRM chunks
    let mut gamma = None;
    if let Some(g) = info.gamma() {
        // g is ScaledFloat, convert to f32 gamma value
        gamma = Some(g.into_value());
    }
    if let Some(chrm) = info.chromaticities() {
        // chrm contains (wx, wy, rx, ry, gx, gy, bx, by)
        // We could try to match against known primaries
        // For simplicity, assume sRGB primaries if chromaticities match closely
        let known = [
            (RgbPrimaries::Bt709, (0.3127, 0.3290, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06)),
            (RgbPrimaries::Adobe1998, (0.3127, 0.3290, 0.64, 0.33, 0.21, 0.71, 0.15, 0.06)),
            (RgbPrimaries::P3, (0.3127, 0.3290, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060)),
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
                    .and_then(|g| {
                        if (g - 1.0/2.2).abs() < 0.01 { Some(TransferFn::Gamma22) }
                        else if (g - 1.0/2.4).abs() < 0.01 { Some(TransferFn::Gamma24) }
                        else if (g - 1.0/2.2).abs() < 0.05 { Some(TransferFn::Gamma22) } // approximate
                        else { None }
                    })
                    .unwrap_or(TransferFn::SrgbGamma);
                return ColorSpace::new(*prim, WhitePoint::D65, transfer);
            }
        }
        // If chromaticities present but no match, assume sRGB with given gamma
        if let Some(g) = gamma {
            let transfer = if (g - 1.0/2.2).abs() < 0.01 {
                TransferFn::Gamma22
            } else if (g - 1.0/2.4).abs() < 0.01 {
                TransferFn::Gamma24
            } else {
                TransferFn::Gamma22 // fallback
            };
            return ColorSpace::new(RgbPrimaries::Bt709, WhitePoint::D65, transfer);
        }
    }
    
    // Priority 5: gAMA alone
    if let Some(g) = gamma {
        let transfer = if (g - 1.0/2.2).abs() < 0.01 {
            TransferFn::Gamma22
        } else if (g - 1.0/2.4).abs() < 0.01 {
            TransferFn::Gamma24
        } else {
            TransferFn::Gamma22 // fallback
        };
        return ColorSpace::new(RgbPrimaries::Bt709, WhitePoint::D65, transfer);
    }
    
    // No color info → assume sRGB (with warning)
    tracing::warn!("No color space metadata in PNG, assuming sRGB");
    ColorSpace::SRGB
}

/// Saves a raw image as PNG to a file path.
pub fn save_png(raw: &RawImage, path: &std::path::Path) -> Result<(), Error> {
    // Determine PNG color type and bit depth
    let (color_type, bit_depth) = match (raw.sample_type.clone(), raw.channel_layout.clone()) {
        (SampleType::U8, ChannelLayoutKind::Rgb) => (ColorType::Rgb, BitDepth::Eight),
        (SampleType::U8, ChannelLayoutKind::Rgba) => (ColorType::Rgba, BitDepth::Eight),
        (SampleType::U8, ChannelLayoutKind::Gray) => (ColorType::Grayscale, BitDepth::Eight),
        (SampleType::U8, ChannelLayoutKind::GrayAlpha) => (ColorType::GrayscaleAlpha, BitDepth::Eight),
        (SampleType::U16, ChannelLayoutKind::Rgb) => (ColorType::Rgb, BitDepth::Sixteen),
        (SampleType::U16, ChannelLayoutKind::Rgba) => (ColorType::Rgba, BitDepth::Sixteen),
        (SampleType::U16, ChannelLayoutKind::Gray) => (ColorType::Grayscale, BitDepth::Sixteen),
        (SampleType::U16, ChannelLayoutKind::GrayAlpha) => (ColorType::GrayscaleAlpha, BitDepth::Sixteen),
        _ => return Err(Error::unsupported_sample_type(
            format!("Unsupported sample type/channel layout for PNG: {:?}/{:?}", raw.sample_type, raw.channel_layout)
        )),
    };

    // PNG expects interleaved samples; convert planar data if needed.
    let data = match raw.sample_layout {
        SampleLayout::Interleaved => raw.data.clone(),
        SampleLayout::Planar => {
            // Convert planar to interleaved
            let channels = raw.channel_layout.channel_count();
            let samples_per_channel = raw.data.len() / channels;
            let mut interleaved = Vec::with_capacity(raw.data.len());
            for i in 0..samples_per_channel {
                for c in 0..channels {
                    interleaved.push(raw.data[c * samples_per_channel + i]);
                }
            }
            interleaved
        }
    };

    let file = File::create(path).map_err(Error::Io)?;
    let w = std::io::BufWriter::new(file);

    let mut encoder = Encoder::new(w, raw.width as u32, raw.height as u32);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);

    // TODO: write color metadata chunks (sRGB / cHRM / cICP)
    let mut writer = encoder.write_header().map_err(|e| Error::Png(e.to_string()))?;
    writer.write_image_data(&data).map_err(|e| Error::Png(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    #[ignore]
    fn roundtrip_save_load() {
        // Create a simple raw image
        let raw = RawImage::new(
            2,
            2,
            SampleType::U8,
            ChannelLayoutKind::Rgb,
            SampleLayout::Interleaved,
            ColorSpace::SRGB,
            AlphaMode::Opaque,
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255], // 4 pixels
        ).unwrap();

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        save_png(&raw, path).unwrap();

        let loaded = load_png(path).unwrap();
        assert_eq!(loaded.width, raw.width);
        assert_eq!(loaded.height, raw.height);
        assert_eq!(loaded.sample_type, raw.sample_type);
        assert_eq!(loaded.channel_layout, raw.channel_layout);
        assert_eq!(loaded.sample_layout, raw.sample_layout);
        assert_eq!(loaded.color_space, raw.color_space);
        assert_eq!(loaded.alpha_mode, raw.alpha_mode);
        assert_eq!(loaded.data, raw.data);
    }
}