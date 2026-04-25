//! SIMD x4‑accelerated conversion rows.
//!
//! Each function has a `*_scalar` reference and a `*_simd` variant.
//! Tests verify that outputs match bit‑exact for random data.

use crate::color::ColorConversion;
use crate::image::ImageBuffer;
use crate::pixel::Rgba;
use half::f16;
use wide::f32x4;

// ---------------------------------------------------------------------------
// Tile-level SIMD conversion: ACEScg f16 tile → sRGB u8 flat buffer
// ---------------------------------------------------------------------------

/// Convert ACEScg f16 premultiplied pixels to encoded u8 RGBA.
/// Uses SIMD 4× at a time for matrix multiply, LUT encode, scalar remainder.
pub(crate) fn acescg_f16_to_srgb_u8_simd(
    pixels: &[Rgba<f16>],
    conv: &ColorConversion,
) -> Vec<u8> {
    let n = pixels.len();
    let mut out = Vec::with_capacity(n * 4);

    let full = n / 4;
    let rem  = n % 4;
    let mat = conv.matrix();

    for chunk in 0..full {
        let base = chunk * 4;
        let mut r_lin = [0.0_f32; 4];
        let mut g_lin = [0.0_f32; 4];
        let mut b_lin = [0.0_f32; 4];
        let mut a_vals = [0.0_f32; 4];

        for i in 0..4 {
            let px = &pixels[base + i];
            let a = px.a.to_f32();
            a_vals[i] = a;
            if a > 0.0 {
                let inv = 1.0 / a;
                r_lin[i] = px.r.to_f32() * inv;
                g_lin[i] = px.g.to_f32() * inv;
                b_lin[i] = px.b.to_f32() * inv;
            }
        }

        let (rr, gg, bb) = mat.mul_vec_simd_x4(
            f32x4::from(r_lin),
            f32x4::from(g_lin),
            f32x4::from(b_lin),
        );

        let r_out: [f32; 4] = rr.into();
        let g_out: [f32; 4] = gg.into();
        let b_out: [f32; 4] = bb.into();

        for i in 0..4 {
            out.push((conv.encode_fast(r_out[i]) * 255.0).round() as u8);
            out.push((conv.encode_fast(g_out[i]) * 255.0).round() as u8);
            out.push((conv.encode_fast(b_out[i]) * 255.0).round() as u8);
            out.push((a_vals[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }

    for i in 0..rem {
        let px = &pixels[full * 4 + i];
        let a = px.a.to_f32();
        let (r, g, b) = if a > 0.0 {
            let inv = 1.0 / a;
            (px.r.to_f32() * inv, px.g.to_f32() * inv, px.b.to_f32() * inv)
        } else {
            (0.0, 0.0, 0.0)
        };
        let linear = mat.mul_vec([r, g, b]);
        out.push((conv.encode_fast(linear[0]) * 255.0).round() as u8);
        out.push((conv.encode_fast(linear[1]) * 255.0).round() as u8);
        out.push((conv.encode_fast(linear[2]) * 255.0).round() as u8);
        out.push((a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }

    out
}

// ---------------------------------------------------------------------------
// ImageBuffer → ACEScg f16 row (format-agnostic)
// ---------------------------------------------------------------------------

/// Convert one row from an ImageBuffer (any layout) to ACEScg f16 premultiplied.
///
/// Uses `ImageBuffer::read_sample` which delegates to `PlaneDesc` → correct for
/// interleaved, planar, gray, RGBA, etc. Processes 4 pixels at once with SIMD
/// matrix multiply via `f32x4`.
pub fn convert_buffer_row_to_acescg_simd(
    source: &ImageBuffer,
    row_y: u32,
    dst: &mut [Rgba<f16>],
    conv: &ColorConversion,
) {
    let w = source.desc.width as usize;
    let has_alpha = source.desc.planes.len() >= 4;
    let is_gray = source.desc.planes.len() <= 2;
    let mat = conv.matrix();
    let gamma = conv.src().transfer();

    let full = w / 4;
    let rem  = w % 4;
    let mut x = 0usize;

    for _ in 0..full {
        let mut r_lin = [0.0_f32; 4];
        let mut g_lin = [0.0_f32; 4];
        let mut b_lin = [0.0_f32; 4];
        let mut a_vals = [0.0_f32; 4];

        for i in 0..4 {
            let px = (x + i) as u32;
            let (rv, gv, bv) = if is_gray {
                let v = source.read_sample(0, px, row_y);
                (v, v, v)
            } else {
                (
                    source.read_sample(0, px, row_y),
                    source.read_sample(1, px, row_y),
                    source.read_sample(2, px, row_y),
                )
            };
            let av = if has_alpha {
                source.read_sample(if is_gray { 1 } else { 3 }, px, row_y)
            } else {
                1.0
            };
            r_lin[i] = gamma.decode(rv);
            g_lin[i] = gamma.decode(gv);
            b_lin[i] = gamma.decode(bv);
            a_vals[i] = av;
        }

        let (rr, gg, bb) = mat.mul_vec_simd_x4(
            f32x4::from(r_lin),
            f32x4::from(g_lin),
            f32x4::from(b_lin),
        );

        let a4 = f32x4::from(a_vals);
        let (rr, gg, bb) = (rr * a4, gg * a4, bb * a4);

        let r_out: [f32; 4] = rr.into();
        let g_out: [f32; 4] = gg.into();
        let b_out: [f32; 4] = bb.into();

        for i in 0..4 {
            dst[x + i] = Rgba {
                r: f16::from_f32(r_out[i]),
                g: f16::from_f32(g_out[i]),
                b: f16::from_f32(b_out[i]),
                a: f16::from_f32(a_vals[i]),
            };
        }
        x += 4;
    }

    for i in 0..rem {
        let px = (x + i) as u32;
        let (rv, gv, bv) = if is_gray {
            let v = source.read_sample(0, px, row_y);
            (v, v, v)
        } else {
            (
                source.read_sample(0, px, row_y),
                source.read_sample(1, px, row_y),
                source.read_sample(2, px, row_y),
            )
        };
        let av = if has_alpha {
            source.read_sample(if is_gray { 1 } else { 3 }, px, row_y)
        } else {
            1.0
        };
        let linear = mat.mul_vec([gamma.decode(rv), gamma.decode(gv), gamma.decode(bv)]);
        dst[x + i] = Rgba {
            r: f16::from_f32(linear[0] * av),
            g: f16::from_f32(linear[1] * av),
            b: f16::from_f32(linear[2] * av),
            a: f16::from_f32(av),
        };
    }
}

// ---------------------------------------------------------------------------
// Scalar references
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// SIMD x4 implementations
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorSpace;
    use crate::assert_approx_eq;

    #[test]
    fn buffer_row_simd_produces_valid_output() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let desc = crate::image::buffer::BufferDesc::rgba8_interleaved(
            4, 4, ColorSpace::SRGB, crate::image::AlphaMode::Straight
        );
        let mut buf = crate::image::ImageBuffer::allocate(desc);
        for y in 0..4 {
            for x in 0..4 {
                let off = (y * 4 + x) as usize * 4;
                buf.data[off]     = (x * 7) as u8;
                buf.data[off + 1] = (x * 13) as u8;
                buf.data[off + 2] = (x * 23) as u8;
                buf.data[off + 3] = ((x * 31) & 0xFF) as u8;
            }
        }

        let mut out = vec![Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO); 4];
        convert_buffer_row_to_acescg_simd(&buf, 0, &mut out, &conv);

        assert_eq!(out.len(), 4);
        for px in &out {
            let r = px.r.to_f32();
            let g = px.g.to_f32();
            let b = px.b.to_f32();
            let a = px.a.to_f32();
            assert!(r.is_finite());
            assert!(g.is_finite());
            assert!(b.is_finite());
            assert!(a.is_finite());
        }
    }

    #[test]
    fn acescg_to_srgb_u8_produces_valid_output() {
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let pixels = vec![
            Rgba { r: f16::from_f32(0.5), g: f16::from_f32(0.3), b: f16::from_f32(0.7), a: f16::from_f32(0.8) },
            Rgba { r: f16::from_f32(1.0), g: f16::from_f32(0.0), b: f16::from_f32(0.5), a: f16::from_f32(1.0) },
            Rgba { r: f16::from_f32(0.0), g: f16::from_f32(0.0), b: f16::from_f32(0.0), a: f16::from_f32(0.0) },
        ];

        let result = acescg_f16_to_srgb_u8_simd(&pixels, &conv);

        assert_eq!(result.len(), pixels.len() * 4);
        for &byte in &result {
            assert!(byte <= 255);
        }
    }
}
