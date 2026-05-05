//! Color conversion engine — precomputed LUTs for fast pixel conversion.

use crate::error::Error;
use crate::model::color::matrix::Matrix3x3;
use crate::model::color::sample::SampleFormat;
use crate::model::color::space::ColorSpace;
use crate::model::color::transfer::TransferFn;
use crate::model::pixel::{AlphaPolicy, Pixel};
use wide::f32x4;

/// Precomputed converter between two color spaces.
///
/// Owns the decode and encode LUTs (generated on `new()`, freed on `drop()`).
/// Create once per conversion batch, reuse across all pixels.
#[derive(Clone, Debug)]
pub struct ColorConversion {
    src: ColorSpace,
    dst: ColorSpace,
    matrix: Matrix3x3,
    /// src u8 (0–255) → linear f32. 256 entries × 4 B = 1 KB.
    decode_u8: Box<[f32]>,
    /// linear [0,1] → dst encoded f32. 4096 entries × 4 B = 16 KB. Used with lerp.
    encode: Box<[f32]>,
}

impl ColorConversion {
    pub fn new(src: ColorSpace, dst: ColorSpace) -> Result<Self, Error> {
        let matrix = src.as_linear().matrix_to(dst.as_linear())?;
        let src_tf = src.transfer();
        let src_u8_lut: Box<[f32]> = (0u16..256)
            .map(|i| src_tf.decode(i as f32 / 255.0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let dst_tf = dst.transfer();
        let encode_lut: Box<[f32]> = (0u16..4096)
            .map(|i| dst_tf.encode(i as f32 / 4095.0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            src,
            dst,
            matrix,
            decode_u8: src_u8_lut,
            encode: encode_lut,
        })
    }

    pub fn src(&self) -> ColorSpace {
        self.src
    }
    pub fn dst(&self) -> ColorSpace {
        self.dst
    }
    pub fn matrix(&self) -> &Matrix3x3 {
        &self.matrix
    }
    pub fn encode_lut(&self) -> &[f32] {
        &self.encode
    }

    // pub fn convert_row<D: Pixel>(&self, data: &[u8], desc: &BufferDesc, y: u32, dst: &mut [D], mode: AlphaPolicy) {
    //     self.convert_row_strided(data, desc, y, 0, desc.width, dst, mode)
    // }
    //
    // pub fn convert_row_strided<D: Pixel>(
    //     &self, data: &[u8], desc: &BufferDesc, y: u32, x_start: u32, x_end: u32, dst: &mut [D], mode: AlphaPolicy,
    // ) {
    //     use crate::color::pipeline::*;
    //     match (desc.planes.len(), desc.planes[0].encoding) {
    //         (4, SampleFormat::U8) => run::<RgbaU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
    //         (3, SampleFormat::U8) => run::<RgbU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
    //         (1, SampleFormat::U8) => run::<GrayU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
    //         (2, SampleFormat::U8) => run::<GrayAlphaU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
    //         (4, SampleFormat::U16Le | SampleFormat::U16Be) => run::<RgbaU16Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
    //         _ => run::<GenericReader, D>(self, data, desc, y, x_start, x_end, dst, mode),
    //     }
    // }

    // pub fn convert_region<D: Pixel>(&self, data: &[u8], desc: &BufferDesc, x: u32, y: u32, w: u32, h: u32, mode: AlphaPolicy) -> Vec<D> {
    //     let mut out = vec![D::pack_one([0.0; 4], AlphaPolicy::Straight); (w * h) as usize];
    //     for row in 0..h {
    //         let dst_row = &mut out[(row * w) as usize..((row + 1) * w) as usize];
    //         self.convert_row_strided(data, desc, y + row, x, x + w, dst_row, mode);
    //     }
    //     out
    // }

    // pub fn convert_buffer<D: Pixel + Send>(&self, data: &[u8], desc: &BufferDesc, mode: AlphaPolicy) -> Vec<D> {
    //     let w = desc.width as usize;
    //     let h = desc.height as usize;
    //     let mut out = vec![D::pack_one([0.0; 4], AlphaPolicy::Straight); w * h];
    //     if h <= 256 {
    //         for row in 0..h {
    //             let dst_row = &mut out[row * w..(row + 1) * w];
    //             self.convert_row_strided(data, desc, row as u32, 0, desc.width, dst_row, mode);
    //         }
    //     } else {
    //         use rayon::prelude::*;
    //         out.par_chunks_exact_mut(w).enumerate().for_each(|(row, dst_row)| {
    //             self.convert_row_strided(data, desc, row as u32, 0, desc.width, dst_row, mode);
    //         });
    //     }
    //     out
    // }

    pub fn convert_pixels<S: Pixel, D: Pixel>(&self, src: &[S], mode: AlphaPolicy) -> Vec<D> {
        let n = src.len();

        // Same color space + same pixel type: no conversion needed.
        if self.src == self.dst && std::any::TypeId::of::<S>() == std::any::TypeId::of::<D>() {
            return bytemuck::cast_slice(src).to_vec();
        }

        // Same color space: skip matrix/transfer, just do pixel format conversion.
        if self.src == self.dst {
            let mut out = Vec::with_capacity(n);
            let full = n / 4;
            let rem = n % 4;
            for chunk in 0..full {
                let (r_lin, g_lin, b_lin, a_vals) = S::unpack_x4(&src[chunk * 4..]);
                let mut tmp = [D::pack_one([0.0; 4], mode); 4];
                D::pack_x4(r_lin, g_lin, b_lin, a_vals, mode, &mut tmp);
                out.extend_from_slice(&tmp);
            }
            for i in 0..rem {
                let [rl, gl, bl, a] = src[full * 4 + i].unpack();
                out.push(D::pack_one([rl, gl, bl, a], mode));
            }
            return out;
        }

        let mat = &self.matrix;
        let tf = self.src.transfer();
        let encode_lut = &self.encode;
        let mut out = Vec::with_capacity(n);
        let full = n / 4;
        let rem = n % 4;
        for chunk in 0..full {
            let (r_lin, g_lin, b_lin, a_vals) = S::unpack_x4(&src[chunk * 4..]);
            let (rr, gg, bb) = mat.mul_vec_simd_x4(
                decode_simd(r_lin, tf),
                decode_simd(g_lin, tf),
                decode_simd(b_lin, tf),
            );
            let (rr, gg, bb) = apply_mode(rr, gg, bb, a_vals, mode);
            let r_enc = encode_simd(rr, encode_lut);
            let g_enc = encode_simd(gg, encode_lut);
            let b_enc = encode_simd(bb, encode_lut);
            let mut tmp = [D::pack_one([0.0; 4], mode); 4];
            D::pack_x4(r_enc, g_enc, b_enc, a_vals, mode, &mut tmp);
            out.extend_from_slice(&tmp);
        }
        for i in 0..rem {
            let [rl, gl, bl, a] = src[full * 4 + i].unpack();
            let decoded = [tf.decode(rl), tf.decode(gl), tf.decode(bl)];
            let linear = mat.mul_vec(decoded);
            let [r, g, b] = apply_mode_one(linear, a, mode);
            let r_enc = lookup_encode(r.clamp(0.0, 1.0), encode_lut);
            let g_enc = lookup_encode(g.clamp(0.0, 1.0), encode_lut);
            let b_enc = lookup_encode(b.clamp(0.0, 1.0), encode_lut);
            out.push(D::pack_one([r_enc, g_enc, b_enc, a], mode));
        }
        out
    }

    pub fn decode_sample(&self, raw: f32, fmt: SampleFormat) -> f32 {
        match fmt {
            SampleFormat::U8 => self.decode_u8[raw as u8 as usize],
            _ => self.src.transfer().decode(raw),
        }
    }

    #[inline]
    pub fn decode_to_linear(&self, rgb: [f32; 3]) -> [f32; 3] {
        let tf = self.src.transfer();
        self.matrix
            .mul_vec([tf.decode(rgb[0]), tf.decode(rgb[1]), tf.decode(rgb[2])])
    }

    #[inline]
    pub fn decode_u8_to_linear(&self, r: u8, g: u8, b: u8) -> [f32; 3] {
        self.matrix.mul_vec([
            self.decode_u8[r as usize],
            self.decode_u8[g as usize],
            self.decode_u8[b as usize],
        ])
    }

    #[inline]
    pub fn encode_fast(&self, y: f32) -> f32 {
        lookup_encode(y.clamp(0.0, 1.0), &self.encode)
    }
}

#[inline(always)]
pub fn lookup_encode(y: f32, lut: &[f32]) -> f32 {
    let idx = y.clamp(0.0, 1.0) * (lut.len() - 1) as f32;
    let i = idx as usize;
    let frac = idx - i as f32;
    if i + 1 < lut.len() {
        lut[i] + frac * (lut[i + 1] - lut[i])
    } else {
        lut[i]
    }
}

#[inline(always)]
pub fn decode_simd(v: f32x4, tf: TransferFn) -> f32x4 {
    let mut out = [0.0; 4];
    for (i, val) in v.to_array().iter().enumerate() {
        out[i] = tf.decode(*val);
    }
    f32x4::from(out)
}

#[inline(always)]
pub fn encode_simd(v: f32x4, lut: &[f32]) -> f32x4 {
    let mut out = [0.0; 4];
    for (i, val) in v.to_array().iter().enumerate() {
        out[i] = lookup_encode(*val, lut);
    }
    f32x4::from(out)
}

pub fn apply_mode(
    rr: f32x4,
    gg: f32x4,
    bb: f32x4,
    aa: f32x4,
    mode: AlphaPolicy,
) -> (f32x4, f32x4, f32x4) {
    match mode {
        AlphaPolicy::PremultiplyOnPack | AlphaPolicy::OpaqueDrop => (rr * aa, gg * aa, bb * aa),
        AlphaPolicy::Straight => (rr, gg, bb),
    }
}

pub fn apply_mode_one(linear: [f32; 3], a: f32, mode: AlphaPolicy) -> [f32; 3] {
    match mode {
        AlphaPolicy::PremultiplyOnPack | AlphaPolicy::OpaqueDrop => {
            [linear[0] * a, linear[1] * a, linear[2] * a]
        }
        AlphaPolicy::Straight => linear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pixel::{AlphaPolicy, Rgba};
    use half::f16;

    #[test]
    fn converter_has_dst_field() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        assert_eq!(conv.dst(), ColorSpace::ACES_CG);
    }

    #[test]
    fn converter_produces_valid_output() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let rgb = [0.5, 0.5, 0.5];
        let result = conv.decode_to_linear(rgb);
        assert!(result.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn encode_fast_valid() {
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        for y in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let encoded = conv.encode_fast(y);
            assert!(encoded.is_finite());
            assert!(encoded >= 0.0 && encoded <= 1.0);
        }
    }

    // #[test]
    // fn convert_row_rgba8_srgb_to_acescg() {
    //     let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
    //     let desc = BufferDesc::rgba8_interleaved(4, 4, ColorSpace::SRGB, AlphaMode::Straight);
    //     let mut buf = ImageBuffer::allocate(desc);
    //     for y in 0..4u32 {
    //         for x in 0..4u32 {
    //             let off = (y * 4 + x) as usize * 4;
    //             buf.data[off] = (x * 7) as u8;
    //             buf.data[off + 1] = (x * 13) as u8;
    //             buf.data[off + 2] = (x * 23) as u8;
    //             buf.data[off + 3] = ((x * 31) & 0xFF) as u8;
    //         }
    //     }
    //     let mut out = vec![Rgba::<f16>::black(); 4];
    //     conv.convert_row::<Rgba<f16>>(&buf.data, &buf.desc, 0, &mut out, AlphaPolicy::PremultiplyOnPack);
    //     assert_eq!(out.len(), 4);
    //     for px in &out { assert!(px.r.to_f32().is_finite()); }
    // }

    // #[test]
    // fn convert_region_and_convert_row_produce_same_output() {
    //     let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
    //     let desc = BufferDesc::rgba8_interleaved(8, 8, ColorSpace::SRGB, AlphaMode::Straight);
    //     let mut buf = ImageBuffer::allocate(desc);
    //     let mut v = 0u8;
    //     for y in 0..8u32 {
    //         for x in 0..8u32 {
    //             let off = (y * 8 + x) as usize * 4;
    //             buf.data[off] = v; buf.data[off + 1] = v.wrapping_add(1);
    //             buf.data[off + 2] = v.wrapping_add(2); buf.data[off + 3] = 255;
    //             v = v.wrapping_add(3);
    //         }
    //     }
    //     let region = conv.convert_region::<Rgba<f16>>(&buf.data, &buf.desc, 2, 1, 4, 3, AlphaPolicy::PremultiplyOnPack);
    //     assert_eq!(region.len(), 12);
    // }

    #[test]
    fn convert_pixels_acescg_to_srgb_u8() {
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let pixels = vec![
            Rgba {
                r: f16::from_f32(0.5),
                g: f16::from_f32(0.3),
                b: f16::from_f32(0.7),
                a: f16::from_f32(0.8),
            },
            Rgba {
                r: f16::from_f32(1.0),
                g: f16::from_f32(0.0),
                b: f16::from_f32(0.5),
                a: f16::from_f32(1.0),
            },
            Rgba {
                r: f16::from_f32(0.0),
                g: f16::from_f32(0.0),
                b: f16::from_f32(0.0),
                a: f16::from_f32(0.0),
            },
        ];
        let result: Vec<[u8; 4]> =
            conv.convert_pixels::<Rgba<f16>, [u8; 4]>(&pixels, AlphaPolicy::Straight);
        assert_eq!(result.len(), pixels.len());
    }

    #[test]
    fn decode_sample_u8_vs_formula() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let val = conv.decode_sample(128.0, SampleFormat::U8);
        let expected = conv.src().transfer().decode(128.0 / 255.0);
        assert!((val - expected).abs() < 1e-5);
    }

    #[test]
    fn decode_sample_f32_unclamped() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let val = conv.decode_sample(2.0, SampleFormat::F32Le);
        assert!(val > 1.0, "HDR float must not be clamped to [0,1]");
    }

    #[test]
    fn convert_pixels_srgb_roundtrip_preserves_values() {
        let srgb_to_acescg = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let acescg_to_srgb = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();

        let colors_f16: Vec<Rgba<f16>> = vec![
            (0.0, 0.0, 0.0),
            (1.0, 1.0, 1.0),
            (0.5, 0.5, 0.5),
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.0, 0.2, 0.8),
        ]
        .into_iter()
        .map(|(r, g, b)| Rgba {
            r: f16::from_f32(r),
            g: f16::from_f32(g),
            b: f16::from_f32(b),
            a: f16::ONE,
        })
        .collect();

        let acescg: Vec<Rgba<f16>> = srgb_to_acescg
            .convert_pixels::<Rgba<f16>, Rgba<f16>>(&colors_f16, AlphaPolicy::PremultiplyOnPack);

        let result: Vec<[u8; 4]> =
            acescg_to_srgb.convert_pixels::<Rgba<f16>, [u8; 4]>(&acescg, AlphaPolicy::Straight);

        for (i, (orig, out)) in colors_f16.iter().zip(result.iter()).enumerate() {
            let expected_r = (orig.r.to_f32().clamp(0.0, 1.0) * 255.0).round() as u8;
            let expected_g = (orig.g.to_f32().clamp(0.0, 1.0) * 255.0).round() as u8;
            let expected_b = (orig.b.to_f32().clamp(0.0, 1.0) * 255.0).round() as u8;
            let expected_a = (orig.a.to_f32().clamp(0.0, 1.0) * 255.0).round() as u8;

            let dr = (out[0] as i32 - expected_r as i32).unsigned_abs();
            let dg = (out[1] as i32 - expected_g as i32).unsigned_abs();
            let db = (out[2] as i32 - expected_b as i32).unsigned_abs();
            let da = (out[3] as i32 - expected_a as i32).unsigned_abs();

            assert!(
                dr <= 2 && dg <= 2 && db <= 2,
                "Color #{i}: expected ({expected_r},{expected_g},{expected_b}) got ({},{},{}) — diff ({dr},{dg},{db})",
                out[0],
                out[1],
                out[2]
            );
            assert_eq!(da, 0, "Alpha #{i}: expected {expected_a} got {}", out[3]);
        }
    }

    #[test]
    fn convert_pixels_same_space_same_type_is_copy() {
        let conv = ColorSpace::ACES_CG
            .converter_to(ColorSpace::ACES_CG)
            .unwrap();

        let src: Vec<Rgba<f16>> = (0..20)
            .map(|i| Rgba {
                r: f16::from_f32(i as f32 / 20.0),
                g: f16::from_f32(0.3),
                b: f16::from_f32(0.7),
                a: f16::ONE,
            })
            .collect();

        let dst: Vec<Rgba<f16>> =
            conv.convert_pixels::<Rgba<f16>, Rgba<f16>>(&src, AlphaPolicy::PremultiplyOnPack);

        assert_eq!(dst.len(), src.len());
        for (i, (s, d)) in src.iter().zip(dst.iter()).enumerate() {
            assert_eq!(s.r.to_bits(), d.r.to_bits(), "r bit mismatch at {i}");
            assert_eq!(s.g.to_bits(), d.g.to_bits(), "g bit mismatch at {i}");
            assert_eq!(s.b.to_bits(), d.b.to_bits(), "b bit mismatch at {i}");
            assert_eq!(s.a.to_bits(), d.a.to_bits(), "a bit mismatch at {i}");
        }
    }

    #[test]
    fn convert_pixels_apply_mode_premultiplies_vs_straight() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();

        let src: Vec<Rgba<f16>> = vec![Rgba {
            r: f16::from_f32(0.25),
            g: f16::from_f32(0.15),
            b: f16::from_f32(0.0),
            a: f16::from_f32(0.5),
        }];

        let premul: Vec<Rgba<f16>> =
            conv.convert_pixels::<Rgba<f16>, Rgba<f16>>(&src, AlphaPolicy::PremultiplyOnPack);
        let straight: Vec<Rgba<f16>> =
            conv.convert_pixels::<Rgba<f16>, Rgba<f16>>(&src, AlphaPolicy::Straight);

        let p = &premul[0];
        let s = &straight[0];

        assert!(
            (p.a.to_f32() - 0.5).abs() < 0.01,
            "alpha preserved in premul: {}",
            p.a.to_f32()
        );
        assert!(
            (s.a.to_f32() - 0.5).abs() < 0.01,
            "alpha preserved in straight: {}",
            s.a.to_f32()
        );

        // With Straight, Rgba<f16>::unpack div-by-alpha is NOT reversed by pack.
        // RGB values should be lower in premul (multiplied by alpha)
        assert!(
            p.r.to_f32() < s.r.to_f32(),
            "premultiplied r ({:.3}) should be < straight r ({:.3})",
            p.r.to_f32(),
            s.r.to_f32()
        );
    }

    #[test]
    fn convert_pixels_rgb_to_rgba_f16_preserves_values() {
        use crate::model::pixel::Rgb;
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::SRGB).unwrap();

        // sRGB green in Rgb<f16>: (0, 1, 0)
        let src: Vec<Rgb<f16>> = vec![
            Rgb {
                r: f16::from_f32(0.0),
                g: f16::from_f32(1.0),
                b: f16::from_f32(0.0),
            },
            Rgb {
                r: f16::from_f32(0.5),
                g: f16::from_f32(0.5),
                b: f16::from_f32(0.5),
            },
        ];

        let dst: Vec<Rgba<f16>> =
            conv.convert_pixels::<Rgb<f16>, Rgba<f16>>(&src, AlphaPolicy::Straight);

        assert_eq!(dst.len(), 2);
        let first = &dst[0];
        assert!(
            (first.r.to_f32() - 0.0).abs() < 0.02,
            "r: {:.3}",
            first.r.to_f32()
        );
        assert!(
            (first.g.to_f32() - 1.0).abs() < 0.02,
            "g: {:.3}",
            first.g.to_f32()
        );
        assert!(
            (first.b.to_f32() - 0.0).abs() < 0.02,
            "b: {:.3}",
            first.b.to_f32()
        );
        assert!(
            (first.a.to_f32() - 1.0).abs() < 0.01,
            "a should be 1.0 from Rgb unpack: {:.3}",
            first.a.to_f32()
        );

        let second = &dst[1];
        assert!(
            (second.a.to_f32() - 1.0).abs() < 0.01,
            "a should be 1.0: {:.3}",
            second.a.to_f32()
        );
    }

    #[test]
    fn convert_pixels_u8_to_f16_rt_alpha_straight() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::SRGB).unwrap();

        let src: Vec<[u8; 4]> = vec![[255, 0, 0, 128], [0, 255, 0, 255], [128, 128, 128, 64]];

        let f16: Vec<Rgba<f16>> =
            conv.convert_pixels::<[u8; 4], Rgba<f16>>(&src, AlphaPolicy::Straight);

        assert_eq!(f16.len(), 3);
        // u8→f16 Straight: values scaled to [0,1], alpha preserved as-is
        assert!(
            (f16[0].r.to_f32() - 1.0).abs() < 0.02,
            "r: {:.3}",
            f16[0].r.to_f32()
        );
        assert!(
            (f16[0].a.to_f32() - 128.0 / 255.0).abs() < 0.02,
            "a: {:.3}",
            f16[0].a.to_f32()
        );
        assert!(
            (f16[1].a.to_f32() - 1.0).abs() < 0.01,
            "a: {:.3}",
            f16[1].a.to_f32()
        );
        assert!(
            (f16[2].a.to_f32() - 64.0 / 255.0).abs() < 0.02,
            "a: {:.3}",
            f16[2].a.to_f32()
        );
    }

    #[test]
    fn convert_pixels_u8_to_f16_roundtrip_with_premul() {
        let to_acescg = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let to_srgb = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();

        let src: Vec<[u8; 4]> = vec![[200, 100, 50, 255], [0, 128, 255, 128]];

        let acescg: Vec<Rgba<f16>> =
            to_acescg.convert_pixels::<[u8; 4], Rgba<f16>>(&src, AlphaPolicy::PremultiplyOnPack);

        let back: Vec<[u8; 4]> =
            to_srgb.convert_pixels::<Rgba<f16>, [u8; 4]>(&acescg, AlphaPolicy::Straight);

        assert_eq!(back.len(), 2);
        // u8 roundtrip through ACEScg should approximately preserve values
        for i in 0..2 {
            let dr = (back[i][0] as i32 - src[i][0] as i32).unsigned_abs();
            let dg = (back[i][1] as i32 - src[i][1] as i32).unsigned_abs();
            let db = (back[i][2] as i32 - src[i][2] as i32).unsigned_abs();
            let da = (back[i][3] as i32 - src[i][3] as i32).unsigned_abs();
            assert!(
                dr <= 2,
                "pixel {i} r diff {dr}: {} vs {}",
                back[i][0],
                src[i][0]
            );
            assert!(dg <= 2, "pixel {i} g diff {dg}");
            assert!(db <= 2, "pixel {i} b diff {db}");
            assert_eq!(da, 0, "pixel {i} alpha should be exact");
        }
    }
}
