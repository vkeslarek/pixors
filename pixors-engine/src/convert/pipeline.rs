//! Conversion pipeline: raw (abstract) images ↔ typed (concrete) working images.
//!
//! Working format: `TypedImage<Rgba<f16>>` — ACEScg linear premultiplied.
//! Uses `ColorConversion` for all color math (LUTs owned per conversion, freed after).

use crate::error::Error;
use crate::color::{ColorSpace, ColorConversion};
use crate::image::{RawImage, TypedImage};
use crate::pixel::Rgba;
use half::f16;
use wide::f32x4;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert a raw image to `TypedImage` in ACEScg premultiplied.
///
/// Creates a `ColorConversion` for the source → ACEScg pair, generating LUTs
/// once per call (freed on return). Supports any `ColorSpace` and `TransferFn`.
///
/// Deprecated: Use `TypedImage::from_raw` directly for zero-copy lazy conversion.
pub fn convert_raw_to_typed(raw: RawImage) -> Result<TypedImage<Rgba<f16>>, Error> {
    let _sw = crate::debug_stopwatch!("convert_raw_to_typed");
    // RawImage is now ImageBuffer; wrap in Arc and create lazy view
    TypedImage::from_raw(std::sync::Arc::new(raw))
}

/// Convert `TypedImage` (ACEScg premultiplied) to sRGB u8 RGBA.
///
/// Creates a `ColorConversion` for ACEScg → sRGB; freed on return.
pub fn convert_acescg_premul_to_srgb_u8(image: &TypedImage<Rgba<f16>>) -> Vec<u8> {
    let _sw = crate::debug_stopwatch!("convert_acescg_premul_to_srgb_u8");

    let conv = ColorSpace::ACES_CG
        .converter_to(ColorSpace::SRGB)
        .expect("ACEScg → sRGB conversion is always valid");

    let pixels = image.read_region(0, 0, image.width(), image.height());
    convert_acescg_premul_pixels_to_srgb_u8(&pixels, &conv)
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

    let pixels = image.read_region(x, y, width, height);
    convert_acescg_premul_pixels_to_srgb_u8(&pixels, conv)
}

/// Convert ACEScg premultiplied pixels to sRGB u8 RGBA.
/// Uses SIMD x4 matrix multiplication via `wide` and scalar fallback per lane.
pub fn convert_acescg_premul_pixels_to_srgb_u8(
    pixels: &[Rgba<f16>],
    conv: &ColorConversion,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 4);

    let chunks = pixels.chunks_exact(4);
    let rem = chunks.remainder();

    for chunk in chunks {
        let mut r = [0.0_f32; 4];
        let mut g = [0.0_f32; 4];
        let mut b = [0.0_f32; 4];
        let mut a = [0.0_f32; 4];

        for (i, px) in chunk.iter().enumerate() {
            let alpha = px.a.to_f32();
            a[i] = alpha;
            if alpha > 0.0 {
                let inv = 1.0 / alpha;
                r[i] = px.r.to_f32() * inv;
                g[i] = px.g.to_f32() * inv;
                b[i] = px.b.to_f32() * inv;
            }
        }

        let (lr, lg, lb) = conv
            .matrix()
            .mul_vec_simd_x4(f32x4::from(r), f32x4::from(g), f32x4::from(b));

        let lr: [f32; 4] = lr.into();
        let lg: [f32; 4] = lg.into();
        let lb: [f32; 4] = lb.into();

        for i in 0..4 {
            out.push((conv.encode_fast(lr[i]) * 255.0).round() as u8);
            out.push((conv.encode_fast(lg[i]) * 255.0).round() as u8);
            out.push((conv.encode_fast(lb[i]) * 255.0).round() as u8);
            out.push((a[i].clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }

    for px in rem {
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


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{ImageBuffer, AlphaMode};
    use crate::image::buffer::BufferDesc;
    use crate::assert_approx_eq;

    #[test]
    fn u8_rgb_roundtrip() {
        let desc = BufferDesc::rgb8_interleaved(1, 1, ColorSpace::SRGB, AlphaMode::Opaque);
        let raw = ImageBuffer {
            desc,
            data: vec![255, 128, 0],
        };
        let typed = convert_raw_to_typed(raw).unwrap();
        assert_eq!(typed.width(), 1);
        assert_eq!(typed.height(), 1);
        let px = typed.read_pixel(0, 0);
        assert!(px.a.to_f32() > 0.99); // opaque
    }

    #[test]
    fn u8_rgba_alpha_premultiplied() {
        let desc = BufferDesc::rgba8_interleaved(1, 1, ColorSpace::SRGB, AlphaMode::Straight);
        let raw = ImageBuffer {
            desc,
            data: vec![255, 0, 0, 128], // red, 50% alpha
        };
        let typed = convert_raw_to_typed(raw).unwrap();
        let px = typed.read_pixel(0, 0);
        let a = px.a.to_f32();
        assert_approx_eq!(a, 128.0 / 255.0, 1e-3);
        // Premultiplied: r should be ≤ a (in linear space, red * a)
        assert!(px.r.to_f32() <= a + 0.01);
    }

    #[test]
    fn any_colorspace_works() {
        // DCI-P3 (Gamma26) should work without panicking.
        let desc = BufferDesc::rgb8_interleaved(1, 1, ColorSpace::DCI_P3, AlphaMode::Opaque);
        let raw = ImageBuffer {
            desc,
            data: vec![200, 100, 50],
        };
        let result = convert_raw_to_typed(raw);
        assert!(result.is_ok());
    }

    #[test]
    fn acescg_to_srgb_roundtrip() {
        let desc = BufferDesc::rgb8_interleaved(1, 1, ColorSpace::SRGB, AlphaMode::Opaque);
        let raw = ImageBuffer {
            desc,
            data: vec![128, 128, 128],
        };
        let typed = convert_raw_to_typed(raw).unwrap();
        let srgb = convert_acescg_premul_to_srgb_u8(&typed);
        // Round-trip through different primaries (SRGB→ACEScg→SRGB) won't be bit-exact
        // but should be close to 128.
        assert!((srgb[0] as i32 - 128).abs() < 5);
    }
}
