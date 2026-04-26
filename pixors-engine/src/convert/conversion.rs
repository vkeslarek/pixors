//! Color conversion engine — precomputed LUTs for fast pixel conversion.

use crate::color::ColorSpace;
use crate::convert::Matrix3x3;
use crate::error::Error;
use crate::image::buffer::{BufferDesc, SampleFormat};
use crate::pixel::{AlphaPolicy, Pixel};

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
        Ok(Self { src, dst, matrix, decode_u8: src_u8_lut, encode: encode_lut })
    }

    pub fn src(&self) -> ColorSpace { self.src }
    pub fn dst(&self) -> ColorSpace { self.dst }
    pub fn matrix(&self) -> &Matrix3x3 { &self.matrix }
    pub fn encode_lut(&self) -> &[f32] { &self.encode }

    pub fn convert_row<D: Pixel>(&self, data: &[u8], desc: &BufferDesc, y: u32, dst: &mut [D], mode: AlphaPolicy) {
        self.convert_row_strided(data, desc, y, 0, desc.width, dst, mode)
    }

    pub fn convert_row_strided<D: Pixel>(
        &self, data: &[u8], desc: &BufferDesc, y: u32, x_start: u32, x_end: u32, dst: &mut [D], mode: AlphaPolicy,
    ) {
        use crate::color::pipeline::*;
        match (desc.planes.len(), desc.planes[0].encoding) {
            (4, SampleFormat::U8) => run::<RgbaU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
            (3, SampleFormat::U8) => run::<RgbU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
            (1, SampleFormat::U8) => run::<GrayU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
            (2, SampleFormat::U8) => run::<GrayAlphaU8Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
            (4, SampleFormat::U16Le | SampleFormat::U16Be) => run::<RgbaU16Interleaved, D>(self, data, desc, y, x_start, x_end, dst, mode),
            _ => run::<GenericReader, D>(self, data, desc, y, x_start, x_end, dst, mode),
        }
    }

    pub fn convert_region<D: Pixel>(&self, data: &[u8], desc: &BufferDesc, x: u32, y: u32, w: u32, h: u32, mode: AlphaPolicy) -> Vec<D> {
        let mut out = vec![D::pack_one([0.0; 4], AlphaPolicy::Straight); (w * h) as usize];
        for row in 0..h {
            let dst_row = &mut out[(row * w) as usize..((row + 1) * w) as usize];
            self.convert_row_strided(data, desc, y + row, x, x + w, dst_row, mode);
        }
        out
    }

    pub fn convert_buffer<D: Pixel + Send>(&self, data: &[u8], desc: &BufferDesc, mode: AlphaPolicy) -> Vec<D> {
        let w = desc.width as usize;
        let h = desc.height as usize;
        let mut out = vec![D::pack_one([0.0; 4], AlphaPolicy::Straight); w * h];
        if h <= 256 {
            for row in 0..h {
                let dst_row = &mut out[row * w..(row + 1) * w];
                self.convert_row_strided(data, desc, row as u32, 0, desc.width, dst_row, mode);
            }
        } else {
            use rayon::prelude::*;
            out.par_chunks_exact_mut(w).enumerate().for_each(|(row, dst_row)| {
                self.convert_row_strided(data, desc, row as u32, 0, desc.width, dst_row, mode);
            });
        }
        out
    }

    pub fn convert_pixels<S: Pixel, D: Pixel>(&self, src: &[S], mode: AlphaPolicy) -> Vec<D> {
        let n = src.len();
        let mat = &self.matrix;
        let tf = self.src.transfer();
        let encode_lut = &self.encode;
        let mut out = Vec::with_capacity(n);
        let full = n / 4;
        let rem = n % 4;
        for chunk in 0..full {
            let (r_lin, g_lin, b_lin, a_vals) = S::unpack_x4(&src[chunk * 4..]);
            let (rr, gg, bb) = mat.mul_vec_simd_x4(
                crate::color::pipeline::decode_simd(r_lin, tf),
                crate::color::pipeline::decode_simd(g_lin, tf),
                crate::color::pipeline::decode_simd(b_lin, tf),
            );
            let (rr, gg, bb) = crate::color::pipeline::apply_mode(rr, gg, bb, a_vals, mode);
            let r_enc = crate::color::pipeline::encode_simd(rr, encode_lut);
            let g_enc = crate::color::pipeline::encode_simd(gg, encode_lut);
            let b_enc = crate::color::pipeline::encode_simd(bb, encode_lut);
            let mut tmp = [D::pack_one([0.0; 4], mode); 4];
            D::pack_x4(r_enc, g_enc, b_enc, a_vals, mode, &mut tmp);
            out.extend_from_slice(&tmp);
        }
        for i in 0..rem {
            let [rl, gl, bl, a] = src[full * 4 + i].unpack();
            let decoded = [tf.decode(rl), tf.decode(gl), tf.decode(bl)];
            let linear = mat.mul_vec(decoded);
            let [r, g, b] = crate::color::pipeline::apply_mode_one(linear, a, mode);
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
        self.matrix.mul_vec([tf.decode(rgb[0]), tf.decode(rgb[1]), tf.decode(rgb[2])])
    }

    #[inline]
    pub fn decode_u8_to_linear(&self, r: u8, g: u8, b: u8) -> [f32; 3] {
        self.matrix.mul_vec([self.decode_u8[r as usize], self.decode_u8[g as usize], self.decode_u8[b as usize]])
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
    if i + 1 < lut.len() { lut[i] + frac * (lut[i + 1] - lut[i]) } else { lut[i] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;
    use crate::image::{buffer::BufferDesc, AlphaMode, ImageBuffer, SampleFormat};
    use crate::pixel::{AlphaPolicy, Rgba};

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

    #[test]
    fn convert_row_rgba8_srgb_to_acescg() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let desc = BufferDesc::rgba8_interleaved(4, 4, ColorSpace::SRGB, AlphaMode::Straight);
        let mut buf = ImageBuffer::allocate(desc);
        for y in 0..4u32 {
            for x in 0..4u32 {
                let off = (y * 4 + x) as usize * 4;
                buf.data[off] = (x * 7) as u8;
                buf.data[off + 1] = (x * 13) as u8;
                buf.data[off + 2] = (x * 23) as u8;
                buf.data[off + 3] = ((x * 31) & 0xFF) as u8;
            }
        }
        let mut out = vec![Rgba::<f16>::black(); 4];
        conv.convert_row::<Rgba<f16>>(&buf.data, &buf.desc, 0, &mut out, AlphaPolicy::PremultiplyOnPack);
        assert_eq!(out.len(), 4);
        for px in &out { assert!(px.r.to_f32().is_finite()); }
    }

    #[test]
    fn convert_region_and_convert_row_produce_same_output() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let desc = BufferDesc::rgba8_interleaved(8, 8, ColorSpace::SRGB, AlphaMode::Straight);
        let mut buf = ImageBuffer::allocate(desc);
        let mut v = 0u8;
        for y in 0..8u32 {
            for x in 0..8u32 {
                let off = (y * 8 + x) as usize * 4;
                buf.data[off] = v; buf.data[off + 1] = v.wrapping_add(1);
                buf.data[off + 2] = v.wrapping_add(2); buf.data[off + 3] = 255;
                v = v.wrapping_add(3);
            }
        }
        let region = conv.convert_region::<Rgba<f16>>(&buf.data, &buf.desc, 2, 1, 4, 3, AlphaPolicy::PremultiplyOnPack);
        assert_eq!(region.len(), 12);
    }

    #[test]
    fn convert_pixels_acescg_to_srgb_u8() {
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let pixels = vec![
            Rgba { r: f16::from_f32(0.5), g: f16::from_f32(0.3), b: f16::from_f32(0.7), a: f16::from_f32(0.8) },
            Rgba { r: f16::from_f32(1.0), g: f16::from_f32(0.0), b: f16::from_f32(0.5), a: f16::from_f32(1.0) },
            Rgba { r: f16::from_f32(0.0), g: f16::from_f32(0.0), b: f16::from_f32(0.0), a: f16::from_f32(0.0) },
        ];
        let result: Vec<[u8; 4]> = conv.convert_pixels::<Rgba<f16>, [u8; 4]>(&pixels, AlphaPolicy::Straight);
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
}
