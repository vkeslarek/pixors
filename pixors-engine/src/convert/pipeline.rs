//! Conversion pipeline: raw (abstract) images ↔ typed (concrete) working images.
//!
//! Working format: `TypedImage<Rgba<f16>>` — ACEScg linear premultiplied.
//! Uses `ColorConversion` for all color math (LUTs owned per conversion, freed after).

use crate::error::Error;
use crate::color::{ColorSpace, ColorConversion};
use crate::image::{RawImage, TypedImage, SampleType, ChannelLayoutKind, SampleLayout, AlphaMode};
use crate::pixel::Rgba;
use half::f16;
use rayon::prelude::*;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert a raw image to `TypedImage<Rgba<f16>>` in ACEScg premultiplied.
///
/// Creates a `ColorConversion` for the source → ACEScg pair, generating LUTs
/// once per call (freed on return). Supports any `ColorSpace` and `TransferFn`.
pub fn convert_raw_to_typed(raw: RawImage) -> Result<TypedImage<Rgba<f16>>, Error> {
    let _sw = crate::debug_stopwatch!("convert_raw_to_typed");

    if !matches!(
        raw.channel_layout,
        ChannelLayoutKind::Gray | ChannelLayoutKind::GrayAlpha
            | ChannelLayoutKind::Rgb | ChannelLayoutKind::Rgba
    ) {
        return Err(Error::unsupported_channel_layout(format!("{:?}", raw.channel_layout)));
    }
    if raw.sample_layout != SampleLayout::Interleaved {
        return Err(Error::unsupported_sample_type(
            "only interleaved layout is currently supported",
        ));
    }

    let conv = raw.color_space.converter_to(ColorSpace::ACES_CG)?;
    let has_alpha = raw.has_alpha();
    let pixel_count = raw.pixel_count();

    let pixels = match raw.sample_type {
        SampleType::U8  => convert_u8 (&raw.data, pixel_count, has_alpha, &conv),
        SampleType::U16 => convert_u16(&raw.data, pixel_count, has_alpha, &conv),
        SampleType::F32 => convert_f32(&raw.data, pixel_count, has_alpha, &conv),
        SampleType::F16 => convert_f16(&raw.data, pixel_count, has_alpha, &conv),
        SampleType::U32 => return Err(Error::unsupported_sample_type("U32")),
    }?;

    Ok(make_acescg_image(raw.width, raw.height, pixels))
}

/// Convert `TypedImage<Rgba<f16>>` (ACEScg premultiplied) to sRGB u8 RGBA.
///
/// Creates a `ColorConversion` for ACEScg → sRGB; freed on return.
pub fn convert_acescg_premul_to_srgb_u8(image: &TypedImage<Rgba<f16>>) -> Vec<u8> {
    let _sw = crate::debug_stopwatch!("convert_acescg_premul_to_srgb_u8");

    let conv = ColorSpace::ACES_CG
        .converter_to(ColorSpace::SRGB)
        .expect("ACEScg → sRGB conversion is always valid");

    let mut out = Vec::with_capacity(image.pixels.len() * 4);

    for px in &image.pixels {
        let a = px.a.to_f32();
        let (r, g, b) = if a > 0.0 {
            let inv = 1.0 / a;
            (px.r.to_f32() * inv, px.g.to_f32() * inv, px.b.to_f32() * inv)
        } else {
            (0.0, 0.0, 0.0)
        };

        let linear = conv.matrix().mul_vec([r, g, b]);

        out.push((conv.encode_fast(linear[0]) * 255.0).round() as u8);
        out.push((conv.encode_fast(linear[1]) * 255.0).round() as u8);
        out.push((conv.encode_fast(linear[2]) * 255.0).round() as u8);
        out.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    out
}

pub fn convert_acescg_premul_region_to_srgb_u8(
    image: &TypedImage<Rgba<f16>>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    conv: &ColorConversion,
) -> Vec<u8> {
    let _sw = crate::debug_stopwatch!("convert_acescg_premul_region_to_srgb_u8");

    let image_width = image.width;

    (y..(y + height))
        .into_par_iter()
        .flat_map_iter(|ty| {
            let mut row = Vec::with_capacity((width * 4) as usize);
            for tx in x..(x + width) {
                let idx = ty as usize * image_width as usize + tx as usize;
                let px = &image.pixels[idx];
                
                let a = px.a.to_f32();
                let (r, g, b) = if a > 0.0 {
                    let inv = 1.0 / a;
                    (px.r.to_f32() * inv, px.g.to_f32() * inv, px.b.to_f32() * inv)
                } else {
                    (0.0, 0.0, 0.0)
                };

                let linear = conv.matrix().mul_vec([r, g, b]);

                row.push((conv.encode_fast(linear[0]) * 255.0).round() as u8);
                row.push((conv.encode_fast(linear[1]) * 255.0).round() as u8);
                row.push((conv.encode_fast(linear[2]) * 255.0).round() as u8);
                row.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            row
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Per-type conversion helpers
// ---------------------------------------------------------------------------

fn convert_u8(
    data: &[u8],
    pixel_count: usize,
    has_alpha: bool,
    conv: &ColorConversion,
) -> Result<Vec<Rgba<f16>>, Error> {
    let stride = if has_alpha { 4 } else { 3 };
    let mut out = Vec::with_capacity(pixel_count);

    for chunk in data.chunks_exact(stride) {
        let rgb = conv.decode_u8_to_linear(chunk[0], chunk[1], chunk[2]);
        let a   = if has_alpha { chunk[3] as f32 / 255.0 } else { 1.0 };
        out.push(pack_rgba(rgb, a));
    }
    Ok(out)
}

fn convert_u16(
    data: &[u8],
    pixel_count: usize,
    has_alpha: bool,
    conv: &ColorConversion,
) -> Result<Vec<Rgba<f16>>, Error> {
    let stride = if has_alpha { 4 } else { 3 };
    let mut out = Vec::with_capacity(pixel_count);

    // Parse big-endian u16 pairs manually (avoids bytemuck alignment requirements).
    let samples: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();

    for chunk in samples.chunks_exact(stride) {
        let rgb = conv.decode_u16_to_linear(chunk[0], chunk[1], chunk[2]);
        let a   = if has_alpha { chunk[3] as f32 / 65535.0 } else { 1.0 };
        out.push(pack_rgba(rgb, a));
    }
    Ok(out)
}

fn convert_f32(
    data: &[u8],
    pixel_count: usize,
    has_alpha: bool,
    conv: &ColorConversion,
) -> Result<Vec<Rgba<f16>>, Error> {
    let stride = if has_alpha { 4 } else { 3 };
    let mut out = Vec::with_capacity(pixel_count);

    let samples: Vec<f32> = data
        .chunks_exact(4)
        .map(|c| f32::from_be_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    for chunk in samples.chunks_exact(stride) {
        let rgb = conv.decode_to_linear([chunk[0], chunk[1], chunk[2]]);
        let a   = if has_alpha { chunk[3].clamp(0.0, 1.0) } else { 1.0 };
        out.push(pack_rgba(rgb, a));
    }
    Ok(out)
}

fn convert_f16(
    data: &[u8],
    pixel_count: usize,
    has_alpha: bool,
    conv: &ColorConversion,
) -> Result<Vec<Rgba<f16>>, Error> {
    let stride = if has_alpha { 4 } else { 3 };
    let mut out = Vec::with_capacity(pixel_count);

    let samples: Vec<f32> = data
        .chunks_exact(2)
        .map(|c| f16::from_bits(u16::from_be_bytes([c[0], c[1]])).to_f32())
        .collect();

    for chunk in samples.chunks_exact(stride) {
        let rgb = conv.decode_to_linear([chunk[0], chunk[1], chunk[2]]);
        let a   = if has_alpha { chunk[3].clamp(0.0, 1.0) } else { 1.0 };
        out.push(pack_rgba(rgb, a));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline(always)]
fn pack_rgba(rgb: [f32; 3], a: f32) -> Rgba<f16> {
    Rgba {
        r: f16::from_f32(rgb[0] * a),
        g: f16::from_f32(rgb[1] * a),
        b: f16::from_f32(rgb[2] * a),
        a: f16::from_f32(a),
    }
}

fn make_acescg_image(width: u32, height: u32, pixels: Vec<Rgba<f16>>) -> TypedImage<Rgba<f16>> {
    TypedImage {
        width,
        height,
        pixels,
        color_space: ColorSpace::ACES_CG,
        alpha_mode:  AlphaMode::Premultiplied,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{RawImage, SampleType, ChannelLayoutKind, SampleLayout, AlphaMode};
    use crate::assert_approx_eq;

    #[test]
    fn u8_rgb_roundtrip() {
        let raw = RawImage::new(
            1, 1,
            SampleType::U8, ChannelLayoutKind::Rgb, SampleLayout::Interleaved,
            ColorSpace::SRGB, AlphaMode::Opaque,
            vec![255, 128, 0],
        ).unwrap();
        let typed = convert_raw_to_typed(raw).unwrap();
        assert_eq!(typed.width, 1);
        assert_eq!(typed.height, 1);
        assert_eq!(typed.pixels.len(), 1);
        assert!(typed.pixels[0].a.to_f32() > 0.99); // opaque
    }

    #[test]
    fn u8_rgba_alpha_premultiplied() {
        let raw = RawImage::new(
            1, 1,
            SampleType::U8, ChannelLayoutKind::Rgba, SampleLayout::Interleaved,
            ColorSpace::SRGB, AlphaMode::Straight,
            vec![255, 0, 0, 128], // red, 50% alpha
        ).unwrap();
        let typed = convert_raw_to_typed(raw).unwrap();
        let px = typed.pixels[0];
        let a = px.a.to_f32();
        assert_approx_eq!(a, 128.0 / 255.0, 1e-3);
        // Premultiplied: r should be ≤ a (in linear space, red * a)
        assert!(px.r.to_f32() <= a + 0.01);
    }

    #[test]
    fn any_colorspace_works() {
        // DCI-P3 (Gamma26) should work without panicking.
        let raw = RawImage::new(
            1, 1,
            SampleType::U8, ChannelLayoutKind::Rgb, SampleLayout::Interleaved,
            ColorSpace::DCI_P3, AlphaMode::Opaque,
            vec![200, 100, 50],
        ).unwrap();
        let result = convert_raw_to_typed(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn acescg_to_srgb_roundtrip() {
        use crate::color::ColorSpace;
        // A linear mid-gray in ACEScg should come back as roughly 0.5 sRGB.
        let raw = RawImage::new(
            1, 1,
            SampleType::U8, ChannelLayoutKind::Rgb, SampleLayout::Interleaved,
            ColorSpace::SRGB, AlphaMode::Opaque,
            vec![128, 128, 128],
        ).unwrap();
        let typed = convert_raw_to_typed(raw).unwrap();
        let srgb = convert_acescg_premul_to_srgb_u8(&typed);
        // Round-trip through different primaries (SRGB→ACEScg→SRGB) won't be bit-exact
        // but should be close to 128.
        assert!((srgb[0] as i32 - 128).abs() < 5);
    }
}
