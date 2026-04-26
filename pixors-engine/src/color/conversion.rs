//! Color space definition and conversions.

use super::{
    matrix, primaries::{RgbPrimaries, WhitePoint},
    transfer::TransferFn,
    Matrix3x3,
};
use crate::convert::pipeline::{self};
use crate::error::Error;
use crate::image::{buffer::SampleFormat, ImageBuffer};
use crate::pixel::{AlphaPolicy, Pixel};

/// A color space: primaries + white point + transfer function.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ColorSpace {
    primaries: RgbPrimaries,
    white_point: WhitePoint,
    transfer: TransferFn,
}

impl ColorSpace {
    pub const fn new(
        primaries: RgbPrimaries,
        white_point: WhitePoint,
        transfer: TransferFn,
    ) -> Self {
        Self {
            primaries,
            white_point,
            transfer,
        }
    }

    pub const fn linear(primaries: RgbPrimaries, white_point: WhitePoint) -> Self {
        Self::new(primaries, white_point, TransferFn::Linear)
    }

    pub const fn primaries(self) -> RgbPrimaries {
        self.primaries
    }
    pub const fn white_point(self) -> WhitePoint {
        self.white_point
    }
    pub const fn transfer(self) -> TransferFn {
        self.transfer
    }
    pub const fn is_linear(self) -> bool {
        self.transfer.is_linear()
    }

    pub const fn as_linear(self) -> Self {
        Self::new(self.primaries, self.white_point, TransferFn::Linear)
    }
    pub const fn with_transfer(self, tf: TransferFn) -> Self {
        Self::new(self.primaries, self.white_point, tf)
    }
    pub const fn with_primaries(self, p: RgbPrimaries) -> Self {
        Self::new(p, self.white_point, self.transfer)
    }
    pub const fn with_white_point(self, wp: WhitePoint) -> Self {
        Self::new(self.primaries, wp, self.transfer)
    }

    /// Linear-RGB → linear-RGB matrix from `self` to `dst` (transfer functions excluded).
    pub fn matrix_to(self, dst: ColorSpace) -> Result<Matrix3x3, Error> {
        if self.primaries == dst.primaries && self.white_point == dst.white_point {
            return Ok(Matrix3x3::IDENTITY);
        }
        matrix::rgb_to_rgb_transform(
            self.primaries,
            self.white_point,
            dst.primaries,
            dst.white_point,
        )
    }

    /// Build a `ColorConversion` from `self` to `dst`.
    /// LUTs are generated on construction and freed when the converter is dropped.
    pub fn converter_to(self, dst: ColorSpace) -> Result<ColorConversion, Error> {
        ColorConversion::new(self, dst)
    }

    // -----------------------------------------------------------------------
    // Predefined color spaces
    // -----------------------------------------------------------------------
    pub const SRGB: Self = Self::new(RgbPrimaries::Bt709, WhitePoint::D65, TransferFn::SrgbGamma);
    pub const LINEAR_SRGB: Self = Self::linear(RgbPrimaries::Bt709, WhitePoint::D65);
    pub const REC709: Self = Self::new(
        RgbPrimaries::Bt709,
        WhitePoint::D65,
        TransferFn::Rec709Gamma,
    );
    pub const REC2020: Self = Self::new(
        RgbPrimaries::Bt2020,
        WhitePoint::D65,
        TransferFn::Rec709Gamma,
    );
    pub const LINEAR_REC2020: Self = Self::linear(RgbPrimaries::Bt2020, WhitePoint::D65);
    pub const ADOBE_RGB: Self = Self::new(
        RgbPrimaries::Adobe1998,
        WhitePoint::D65,
        TransferFn::Gamma22,
    );
    pub const DISPLAY_P3: Self =
        Self::new(RgbPrimaries::P3, WhitePoint::D65, TransferFn::SrgbGamma);
    pub const LINEAR_DISPLAY_P3: Self = Self::linear(RgbPrimaries::P3, WhitePoint::D65);
    pub const DCI_P3: Self = Self::new(RgbPrimaries::P3, WhitePoint::P3Dci, TransferFn::Gamma26);
    pub const PROPHOTO: Self = Self::new(
        RgbPrimaries::ProPhoto,
        WhitePoint::D50,
        TransferFn::ProPhotoGamma,
    );
    pub const ACES2065_1: Self = Self::linear(RgbPrimaries::Ap0, WhitePoint::D60);
    /// ACEScg — the engine's working color space.
    pub const ACES_CG: Self = Self::linear(RgbPrimaries::Ap1, WhitePoint::D60);
}

// ---------------------------------------------------------------------------
// ColorConversion
// ---------------------------------------------------------------------------

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
    /// Build a converter. Generates LUTs immediately (~17 KB per converter).
    pub fn new(src: ColorSpace, dst: ColorSpace) -> Result<Self, Error> {
        let matrix = src.as_linear().matrix_to(dst.as_linear())?;
        Ok(Self {
            src,
            dst,
            matrix,
            decode_u8: build_decode_u8(src.transfer()),
            encode: build_encode(dst.transfer()),
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
    /// Exposed for the generic pipeline (encode_simd helper in pipeline.rs).
    pub fn encode_lut(&self) -> &[f32] {
        &self.encode
    }

    // -----------------------------------------------------------------------
    // New universal conversion API (Step 2+3)
    // -----------------------------------------------------------------------

    /// Convert one full row of an ImageBuffer into `dst`.
    /// `dst.len() == buf.desc.width as usize`.
    pub fn convert_row<D: Pixel>(
        &self,
        buf: &ImageBuffer,
        y: u32,
        dst: &mut [D],
        mode: AlphaPolicy,
    ) {
        self.convert_row_strided(buf, y, 0, buf.desc.width, dst, mode)
    }

    /// Convert pixels `x_start..x_end` on one row into `dst`.
    /// `dst.len() == (x_end - x_start) as usize`.
    pub fn convert_row_strided<D: Pixel>(
        &self,
        buf: &ImageBuffer,
        y: u32,
        x_start: u32,
        x_end: u32,
        dst: &mut [D],
        mode: AlphaPolicy,
    ) {
        use pipeline::*;

        match (buf.desc.planes.len(), buf.desc.planes[0].encoding) {
            (4, SampleFormat::U8) => {
                run::<RgbaU8Interleaved, D>(self, buf, y, x_start, x_end, dst, mode)
            }
            (3, SampleFormat::U8) => {
                run::<RgbU8Interleaved, D>(self, buf, y, x_start, x_end, dst, mode)
            }
            (1, SampleFormat::U8) => {
                run::<GrayU8Interleaved, D>(self, buf, y, x_start, x_end, dst, mode)
            }
            (2, SampleFormat::U8) => {
                run::<GrayAlphaU8Interleaved, D>(self, buf, y, x_start, x_end, dst, mode)
            }
            (4, SampleFormat::U16Le | SampleFormat::U16Be) => {
                run::<RgbaU16Interleaved, D>(self, buf, y, x_start, x_end, dst, mode)
            }
            _ => run::<GenericReader, D>(self, buf, y, x_start, x_end, dst, mode),
        }
    }

    /// Convert a rectangular region into a fresh `Vec<D>`.
    pub fn convert_region<D: Pixel>(
        &self,
        buf: &ImageBuffer,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        mode: AlphaPolicy,
    ) -> Vec<D> {
        let mut out = vec![D::pack_one([0.0; 4], AlphaPolicy::Straight); (w * h) as usize];
        for row in 0..h {
            let dst_row = &mut out[(row * w) as usize..((row + 1) * w) as usize];
            self.convert_row_strided(buf, y + row, x, x + w, dst_row, mode);
        }
        out
    }

    /// Convert the entire image into a flat `Vec<D>`. Uses rayon over rows.
    pub fn convert_buffer<D: Pixel + Send>(
        &self,
        buf: &ImageBuffer,
        mode: AlphaPolicy,
    ) -> Vec<D> {
        use rayon::prelude::*;
        let w = buf.desc.width as usize;
        let h = buf.desc.height as usize;
        let mut out = vec![D::pack_one([0.0; 4], AlphaPolicy::Straight); w * h];

        out.par_chunks_exact_mut(w)
            .enumerate()
            .for_each(|(row, dst_row)| {
                self.convert_row_strided(buf, row as u32, 0, buf.desc.width, dst_row, mode);
            });

        out
    }

    /// Convert a flat slice of typed source pixels into `Vec<D>`.
    /// Source pixels are already in the source color space.
    pub fn convert_pixels<S: Pixel, D: Pixel>(
        &self,
        src: &[S],
        mode: AlphaPolicy,
    ) -> Vec<D> {
        let n = src.len();
        let mat = self.matrix();
        let tf = self.src.transfer();
        let encode_lut = &self.encode;
        let mut out = Vec::with_capacity(n);

        let full = n / 4;
        let rem = n % 4;

        for chunk in 0..full {
            let (r_lin, g_lin, b_lin, a_vals) = S::unpack_x4(&src[chunk * 4..]);

            let (rr, gg, bb) = mat.mul_vec_simd_x4(
                pipeline::decode_simd(r_lin, tf),
                pipeline::decode_simd(g_lin, tf),
                pipeline::decode_simd(b_lin, tf),
            );

            let (rr, gg, bb) = pipeline::apply_mode(rr, gg, bb, a_vals, mode);

            let r_enc = pipeline::encode_simd(rr, encode_lut);
            let g_enc = pipeline::encode_simd(gg, encode_lut);
            let b_enc = pipeline::encode_simd(bb, encode_lut);

            let mut tmp = [D::pack_one([0.0; 4], mode); 4];
            D::pack_x4(r_enc, g_enc, b_enc, a_vals, mode, &mut tmp);
            out.extend_from_slice(&tmp);
        }

        for i in 0..rem {
            let [rl, gl, bl, a] = src[full * 4 + i].unpack();
            let decoded = [tf.decode(rl), tf.decode(gl), tf.decode(bl)];
            let linear = mat.mul_vec(decoded);
            let [r, g, b] = pipeline::apply_mode_one(linear, a, mode);
            let r_enc = lookup_encode(r.clamp(0.0, 1.0), encode_lut);
            let g_enc = lookup_encode(g.clamp(0.0, 1.0), encode_lut);
            let b_enc = lookup_encode(b.clamp(0.0, 1.0), encode_lut);
            out.push(D::pack_one([r_enc, g_enc, b_enc, a], mode));
        }

        out
    }

    // -----------------------------------------------------------------------
    // Sample decode (u8 LUT, formula for other formats)
    // -----------------------------------------------------------------------

    /// Decode one raw sample value (0..=255 for u8, normalized for others) to linear f32.
    pub fn decode_sample(&self, raw: f32, fmt: SampleFormat) -> f32 {
        match fmt {
            SampleFormat::U8 => self.decode_u8[raw as u8 as usize],
            _ => self.src.transfer().decode(raw),
        }
    }

    // -----------------------------------------------------------------------
    // Single-pixel / single-value (keep for backward compat until Step 9)
    // -----------------------------------------------------------------------

    /// Decode src gamma + apply matrix → dst-linear RGB.
    /// Does NOT encode dst transfer function. Efficient when dst is linear (e.g., ACEScg).
    #[inline]
    pub fn decode_to_linear(&self, rgb: [f32; 3]) -> [f32; 3] {
        let tf = self.src.transfer();
        self.matrix
            .mul_vec([tf.decode(rgb[0]), tf.decode(rgb[1]), tf.decode(rgb[2])])
    }

    /// Fast: decode 3 u8 values via LUT → dst-linear RGB.
    #[inline]
    pub fn decode_u8_to_linear(&self, r: u8, g: u8, b: u8) -> [f32; 3] {
        self.matrix.mul_vec([
            self.decode_u8[r as usize],
            self.decode_u8[g as usize],
            self.decode_u8[b as usize],
        ])
    }

    /// Encode one linear value to dst encoding via LUT (with linear interpolation).
    /// `y` is clamped to `[0, 1]` before lookup.
    #[inline]
    pub fn encode_fast(&self, y: f32) -> f32 {
        lookup_encode(y.clamp(0.0, 1.0), &self.encode)
    }
}

// ---------------------------------------------------------------------------
// LUT builders
// ---------------------------------------------------------------------------

fn build_decode_u8(tf: TransferFn) -> Box<[f32]> {
    (0u16..256)
        .map(|i| tf.decode(i as f32 / 255.0))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn build_encode(tf: TransferFn) -> Box<[f32]> {
    (0u16..4096)
        .map(|i| tf.encode(i as f32 / 4095.0))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[inline(always)]
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{buffer::BufferDesc, AlphaMode, ImageBuffer, SampleFormat};
    use crate::pixel::{AlphaPolicy, Rgba};
    use half::f16;

    #[test]
    fn color_space_constants() {
        assert_eq!(ColorSpace::SRGB.primaries(), RgbPrimaries::Bt709);
        assert_eq!(ColorSpace::SRGB.white_point(), WhitePoint::D65);
        assert_eq!(ColorSpace::SRGB.transfer(), TransferFn::SrgbGamma);
        assert!(ColorSpace::ACES_CG.is_linear());
    }

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

    // --- New pipeline regression tests ---

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
        conv.convert_row::<Rgba<f16>>(&buf, 0, &mut out, AlphaPolicy::PremultiplyOnPack);

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
    fn convert_region_and_convert_row_produce_same_output() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let desc = BufferDesc::rgba8_interleaved(8, 8, ColorSpace::SRGB, AlphaMode::Straight);
        let mut buf = ImageBuffer::allocate(desc);

        let mut v = 0u8;
        for y in 0..8u32 {
            for x in 0..8u32 {
                let off = (y * 8 + x) as usize * 4;
                buf.data[off] = v;
                buf.data[off + 1] = v.wrapping_add(1);
                buf.data[off + 2] = v.wrapping_add(2);
                buf.data[off + 3] = 255;
                v = v.wrapping_add(3);
            }
        }

        let region =
            conv.convert_region::<Rgba<f16>>(&buf, 2, 1, 4, 3, AlphaPolicy::PremultiplyOnPack);
        assert_eq!(region.len(), 12);

        let mut row_out = vec![Rgba::<f16>::black(); 4];
        for row in 0..3u32 {
            conv.convert_row_strided::<Rgba<f16>>(
                &buf,
                1 + row,
                2,
                6,
                &mut row_out,
                AlphaPolicy::PremultiplyOnPack,
            );
            for i in 0..4 {
                let reg_idx = (row * 4 + i as u32) as usize;
                let (a, b) = (&region[reg_idx], &row_out[i]);
                assert!(
                    (a.r.to_f32() - b.r.to_f32()).abs() < 1e-5,
                    "mismatch at pixel ({}, {})",
                    2 + i,
                    1 + row
                );
            }
        }
    }

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
        for px in &result {
            assert!(px[0] <= 255);
            assert!(px[1] <= 255);
            assert!(px[2] <= 255);
            assert!(px[3] <= 255);
        }
    }

    #[test]
    fn decode_sample_u8_vs_formula() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let val = conv.decode_sample(128.0, SampleFormat::U8);
        let expected = conv.src.transfer().decode(128.0 / 255.0);
        assert!((val - expected).abs() < 1e-5);
    }

    #[test]
    fn decode_sample_f32_unclamped() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let val = conv.decode_sample(2.0, SampleFormat::F32Le);
        assert!(val > 1.0, "HDR float must not be clamped to [0,1]");
    }
}
