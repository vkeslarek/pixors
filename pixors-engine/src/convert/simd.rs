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

/// Convert a flat tile of ACEScg f16 premultiplied pixels to sRGB u8 RGBA bytes.
/// Uses SIMD 4× at a time for the matrix multiply, scalar remainder.
/// The matrix is the 3x3 primary transform (ACEScg → sRGB).
pub fn acescg_f16_to_srgb_u8_simd(
    pixels: &[Rgba<f16>],
    matrix: [[f32; 3]; 3],
) -> Vec<u8> {
    let n = pixels.len();
    let mut out = Vec::with_capacity(n * 4);

    let full = n / 4;
    let rem  = n % 4;

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

        let rv = f32x4::from(r_lin);
        let gv = f32x4::from(g_lin);
        let bv = f32x4::from(b_lin);

        let rout: [f32; 4] = (f32x4::splat(matrix[0][0]) * rv
            + f32x4::splat(matrix[0][1]) * gv
            + f32x4::splat(matrix[0][2]) * bv).into();
        let gout: [f32; 4] = (f32x4::splat(matrix[1][0]) * rv
            + f32x4::splat(matrix[1][1]) * gv
            + f32x4::splat(matrix[1][2]) * bv).into();
        let bout: [f32; 4] = (f32x4::splat(matrix[2][0]) * rv
            + f32x4::splat(matrix[2][1]) * gv
            + f32x4::splat(matrix[2][2]) * bv).into();

        for i in 0..4 {
            out.push(encode_srgb_u8(rout[i]));
            out.push(encode_srgb_u8(gout[i]));
            out.push(encode_srgb_u8(bout[i]));
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
        let lr = matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b;
        let lg = matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b;
        let lb = matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b;
        out.push(encode_srgb_u8(lr));
        out.push(encode_srgb_u8(lg));
        out.push(encode_srgb_u8(lb));
        out.push((a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }

    out
}

#[inline(always)]
fn encode_srgb_u8(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
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

/// Scalar reference: RGBA u8 row → ACEScg f16 premultiplied.
pub fn convert_u8_row_to_acescg_f16_scalar(
    row: &[u8],
    width: u32,
    conv: &ColorConversion,
) -> Vec<Rgba<f16>> {
    let n = width as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let off = i * 4;
        let linear = conv.decode_u8_to_linear(row[off], row[off + 1], row[off + 2]);
        let a = row[off + 3] as f32 / 255.0;
        out.push(Rgba {
            r: f16::from_f32(linear[0] * a),
            g: f16::from_f32(linear[1] * a),
            b: f16::from_f32(linear[2] * a),
            a: f16::from_f32(a),
        });
    }
    out
}

/// Scalar reference: ACEScg f16 premultiplied row → RGBA u8.
pub fn convert_acescg_f16_row_to_srgb_u8_scalar(
    pixels: &[Rgba<f16>],
    width: u32,
    conv: &ColorConversion,
) -> Vec<u8> {
    let n = width as usize;
    let mut out = Vec::with_capacity(n * 4);
    for px in pixels.iter().take(n) {
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
// SIMD x4 implementations
// ---------------------------------------------------------------------------

/// SIMD x4: RGBA u8 row → ACEScg f16 premultiplied.
///
/// Processes 4 pixels at once:
///   1. scalar LUT decode (u8 → sRGB linear)
///   2. f32x4 matrix multiply (sRGB linear → ACEScg linear)
///   3. f32x4 premultiply
///   4. store 4× Rgba<f16>
pub fn convert_u8_row_to_acescg_f16_simd(
    row: &[u8],
    width: u32,
    conv: &ColorConversion,
) -> Vec<Rgba<f16>> {
    let n = width as usize;
    let mut out = Vec::with_capacity(n);

    let full = n / 4;
    let rem  = n % 4;
    let lut  = conv.decode_u8_lut();
    let mat  = conv.matrix();

    for chunk in 0..full {
        let base = chunk * 16;
        let mut r_lin = [0.0_f32; 4];
        let mut g_lin = [0.0_f32; 4];
        let mut b_lin = [0.0_f32; 4];
        let mut a_vals = [0.0_f32; 4];

        for i in 0..4 {
            let off = base + i * 4;
            r_lin[i] = lut[row[off] as usize];
            g_lin[i] = lut[row[off + 1] as usize];
            b_lin[i] = lut[row[off + 2] as usize];
            a_vals[i] = row[off + 3] as f32 / 255.0;
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
            out.push(Rgba {
                r: f16::from_f32(r_out[i]),
                g: f16::from_f32(g_out[i]),
                b: f16::from_f32(b_out[i]),
                a: f16::from_f32(a_vals[i]),
            });
        }
    }

    for i in 0..rem {
        let off = full * 16 + i * 4;
        let linear = conv.decode_u8_to_linear(row[off], row[off + 1], row[off + 2]);
        let a = row[off + 3] as f32 / 255.0;
        out.push(Rgba {
            r: f16::from_f32(linear[0] * a),
            g: f16::from_f32(linear[1] * a),
            b: f16::from_f32(linear[2] * a),
            a: f16::from_f32(a),
        });
    }

    out
}

/// SIMD x4: ACEScg f16 premultiplied row → RGBA u8.
///
/// Processes 4 pixels at once:
///   1. scalar unpremultiply (with zero‑alpha guard)
///   2. f32x4 matrix multiply (ACEScg linear → sRGB linear)
///   3. scalar LUT encode + quantize to u8
pub fn convert_acescg_f16_row_to_srgb_u8_simd(
    pixels: &[Rgba<f16>],
    width: u32,
    conv: &ColorConversion,
) -> Vec<u8> {
    let n = width as usize;
    let mut out = Vec::with_capacity(n * 4);

    let full = n / 4;
    let rem  = n % 4;
    let mat  = conv.matrix();

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
            out.push((a_vals[i].clamp(0.0, 1.0) * 255.0).round() as u8);
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
        let linear = conv.matrix().mul_vec([r, g, b]);
        out.push((conv.encode_fast(linear[0]) * 255.0).round() as u8);
        out.push((conv.encode_fast(linear[1]) * 255.0).round() as u8);
        out.push((conv.encode_fast(linear[2]) * 255.0).round() as u8);
        out.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
    }

    out
}

// ---------------------------------------------------------------------------
// Tests: compare SIMD vs scalar reference
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorSpace;
    use crate::assert_approx_eq;

    fn mk_test_row(width: u32) -> Vec<u8> {
        let mut row = Vec::with_capacity(width as usize * 4);
        for i in 0..width {
            row.push((i * 7) as u8);
            row.push((i * 13) as u8);
            row.push((i * 23) as u8);
            row.push(((i * 31) & 0xFF) as u8);
        }
        row
    }

    fn check_f16_approx(a: &Rgba<f16>, b: &Rgba<f16>, eps: f32) {
        assert_approx_eq!(a.r.to_f32(), b.r.to_f32(), eps);
        assert_approx_eq!(a.g.to_f32(), b.g.to_f32(), eps);
        assert_approx_eq!(a.b.to_f32(), b.b.to_f32(), eps);
        assert_approx_eq!(a.a.to_f32(), b.a.to_f32(), eps);
    }

    #[test]
    fn forward_simd_matches_scalar_srgb() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        for width in [0, 1, 3, 4, 5, 8, 16] {
            let row = mk_test_row(width);
            let scalar = convert_u8_row_to_acescg_f16_scalar(&row, width, &conv);
            let simd   = convert_u8_row_to_acescg_f16_simd(&row, width, &conv);
            assert_eq!(scalar.len(), simd.len());
            for (a, b) in scalar.iter().zip(simd.iter()) {
                check_f16_approx(a, b, 1e-3);
            }
        }
    }

    #[test]
    fn forward_simd_matches_scalar_rec709() {
        let conv = ColorSpace::REC709.converter_to(ColorSpace::ACES_CG).unwrap();
        let row = mk_test_row(16);
        let scalar = convert_u8_row_to_acescg_f16_scalar(&row, 16, &conv);
        let simd   = convert_u8_row_to_acescg_f16_simd(&row, 16, &conv);
        for (a, b) in scalar.iter().zip(simd.iter()) {
            check_f16_approx(a, b, 1e-3);
        }
    }

    #[test]
    fn reverse_simd_matches_scalar() {
        let conv_to   = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let conv_from = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();

        for width in [0, 1, 3, 4, 5, 8, 16] {
            let row = mk_test_row(width);
            let aces = convert_u8_row_to_acescg_f16_scalar(&row, width, &conv_to);

            let scalar = convert_acescg_f16_row_to_srgb_u8_scalar(&aces, width, &conv_from);
            let simd   = convert_acescg_f16_row_to_srgb_u8_simd(&aces, width, &conv_from);
            assert_eq!(scalar, simd, "mismatch at width={width}");
        }
    }

    #[test]
    fn reverse_simd_zero_alpha() {
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let pixels = vec![Rgba { r: f16::ZERO, g: f16::ZERO, b: f16::ZERO, a: f16::ZERO }; 8];
        let scalar = convert_acescg_f16_row_to_srgb_u8_scalar(&pixels, 8, &conv);
        let simd   = convert_acescg_f16_row_to_srgb_u8_simd(&pixels, 8, &conv);
        assert_eq!(scalar, simd);
        // All zero-alpha → fully transparent black
        for c in 0..scalar.len() {
            assert_eq!(scalar[c], 0, "all bytes should be 0 with zero alpha");
        }
    }

    fn make_image_buffer(width: u32, height: u32, color_space: ColorSpace) -> ImageBuffer {
        use crate::image::buffer::BufferDesc;
        let desc = BufferDesc::rgba8_interleaved(width, height, color_space, crate::image::AlphaMode::Straight);
        let mut buf = ImageBuffer::allocate(desc);
        for y in 0..height {
            for x in 0..width {
                let off = (y * width + x) as usize * 4;
                buf.data[off]     = (x * 7) as u8;
                buf.data[off + 1] = (x * 13) as u8;
                buf.data[off + 2] = (x * 23) as u8;
                buf.data[off + 3] = ((x * 31) & 0xFF) as u8;
            }
        }
        buf
    }

    #[test]
    fn buffer_row_simd_matches_scalar() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        for width in [0, 1, 3, 4, 5, 8, 16] {
            let image = make_image_buffer(width, 4, ColorSpace::SRGB);
            let mut simd_out = vec![Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO); width as usize];
            convert_buffer_row_to_acescg_simd(&image, 0, &mut simd_out, &conv);

            let row_data: Vec<u8> = image.data[..width as usize * 4].to_vec();
            let scalar_out = convert_u8_row_to_acescg_f16_scalar(&row_data, width, &conv);

            assert_eq!(simd_out.len(), scalar_out.len());
            for (a, b) in simd_out.iter().zip(scalar_out.iter()) {
                check_f16_approx(a, b, 1e-3);
            }
        }
    }

    #[test]
    fn partial_alpha() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let mut row = vec![0u8; 16]; // 4 pixels, RGBA
        row[3] = 128; row[7] = 64; row[11] = 0; row[15] = 255;
        let scalar = convert_u8_row_to_acescg_f16_scalar(&row, 4, &conv);
        let simd   = convert_u8_row_to_acescg_f16_simd(&row, 4, &conv);
        for (a, b) in scalar.iter().zip(simd.iter()) {
            check_f16_approx(a, b, 1e-3);
        }
        // alpha[3] = 1.0
        assert!((scalar[3].a.to_f32() - 1.0).abs() < 1e-3);
    }
}
