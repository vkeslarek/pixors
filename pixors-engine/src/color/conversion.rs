//! Color space definition and conversions.

use super::{
    matrix,
    primaries::{RgbPrimaries, WhitePoint},
    transfer::TransferFn,
    Matrix3x3,
};
use crate::Error;

/// A color space: primaries + white point + transfer function.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ColorSpace {
    primaries:   RgbPrimaries,
    white_point: WhitePoint,
    transfer:    TransferFn,
}

impl ColorSpace {
    pub const fn new(primaries: RgbPrimaries, white_point: WhitePoint, transfer: TransferFn) -> Self {
        Self { primaries, white_point, transfer }
    }

    pub const fn linear(primaries: RgbPrimaries, white_point: WhitePoint) -> Self {
        Self::new(primaries, white_point, TransferFn::Linear)
    }

    pub const fn primaries(self)   -> RgbPrimaries { self.primaries }
    pub const fn white_point(self) -> WhitePoint   { self.white_point }
    pub const fn transfer(self)    -> TransferFn   { self.transfer }
    pub const fn is_linear(self)   -> bool         { self.transfer.is_linear() }

    pub const fn as_linear(self) -> Self {
        Self::new(self.primaries, self.white_point, TransferFn::Linear)
    }
    pub const fn with_transfer(self, tf: TransferFn)    -> Self { Self::new(self.primaries, self.white_point, tf) }
    pub const fn with_primaries(self, p: RgbPrimaries)  -> Self { Self::new(p, self.white_point, self.transfer) }
    pub const fn with_white_point(self, wp: WhitePoint) -> Self { Self::new(self.primaries, wp, self.transfer) }

    /// Linear-RGB → linear-RGB matrix from `self` to `dst` (transfer functions excluded).
    pub fn matrix_to(self, dst: ColorSpace) -> Result<Matrix3x3, Error> {
        if self.primaries == dst.primaries && self.white_point == dst.white_point {
            return Ok(Matrix3x3::IDENTITY);
        }
        matrix::rgb_to_rgb_transform(self.primaries, self.white_point, dst.primaries, dst.white_point)
    }

    /// Build a `ColorConversion` from `self` to `dst`.
    /// LUTs are generated on construction and freed when the converter is dropped.
    pub fn converter_to(self, dst: ColorSpace) -> Result<ColorConversion, Error> {
        ColorConversion::new(self, dst)
    }

    // -----------------------------------------------------------------------
    // Predefined color spaces
    // -----------------------------------------------------------------------
    pub const SRGB:              Self = Self::new(RgbPrimaries::Bt709,     WhitePoint::D65, TransferFn::SrgbGamma);
    pub const LINEAR_SRGB:       Self = Self::linear(RgbPrimaries::Bt709,  WhitePoint::D65);
    pub const REC709:            Self = Self::new(RgbPrimaries::Bt709,     WhitePoint::D65, TransferFn::Rec709Gamma);
    pub const REC2020:           Self = Self::new(RgbPrimaries::Bt2020,    WhitePoint::D65, TransferFn::Rec709Gamma);
    pub const LINEAR_REC2020:    Self = Self::linear(RgbPrimaries::Bt2020, WhitePoint::D65);
    pub const ADOBE_RGB:         Self = Self::new(RgbPrimaries::Adobe1998, WhitePoint::D65, TransferFn::Gamma22);
    pub const DISPLAY_P3:        Self = Self::new(RgbPrimaries::P3,        WhitePoint::D65, TransferFn::SrgbGamma);
    pub const LINEAR_DISPLAY_P3: Self = Self::linear(RgbPrimaries::P3,     WhitePoint::D65);
    pub const DCI_P3:            Self = Self::new(RgbPrimaries::P3,        WhitePoint::P3Dci, TransferFn::Gamma26);
    pub const PROPHOTO:          Self = Self::new(RgbPrimaries::ProPhoto,  WhitePoint::D50, TransferFn::ProPhotoGamma);
    pub const ACES2065_1:        Self = Self::linear(RgbPrimaries::Ap0,    WhitePoint::D60);
    /// ACEScg — the engine's working color space.
    pub const ACES_CG:           Self = Self::linear(RgbPrimaries::Ap1,    WhitePoint::D60);
}

// ---------------------------------------------------------------------------
// ColorConversion
// ---------------------------------------------------------------------------

/// Precomputed converter between two color spaces.
///
/// Owns its LUT tables — they are generated on `new()` and freed on `drop()`.
/// Create once per conversion batch (e.g., per image), reuse across all pixels.
///
/// # Example
/// ```ignore
/// let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG)?;
///
/// // u8 pixels → ACEScg linear:
/// for chunk in data.chunks_exact(3) {
///     let rgb = conv.decode_u8_to_linear(chunk[0], chunk[1], chunk[2]);
/// }
/// ```
#[derive(Clone)]
pub struct ColorConversion {
    src:    ColorSpace,
    dst:    ColorSpace,
    matrix: Matrix3x3,
    /// src u8 (0–255) → linear f32. 256 entries × 4 B = 1 KB.
    decode_u8:  Box<[f32]>,
    /// src u16 (0–65535) → linear f32. 65536 entries × 4 B = 256 KB.
    decode_u16: Box<[f32]>,
    /// linear [0,1] → dst encoded f32. 4096 entries × 4 B = 16 KB. Used with lerp.
    encode:     Box<[f32]>,
}

impl ColorConversion {
    /// Build a converter. Generates all LUTs immediately (totals ~273 KB per converter).
    pub fn new(src: ColorSpace, dst: ColorSpace) -> Result<Self, Error> {
        let matrix = src.as_linear().matrix_to(dst.as_linear())?;
        Ok(Self {
            src,
            dst,
            matrix,
            decode_u8:  build_decode_u8(src.transfer()),
            decode_u16: build_decode_u16(src.transfer()),
            encode:     build_encode(dst.transfer()),
        })
    }

    pub fn src(&self)    -> ColorSpace    { self.src }
    pub fn dst(&self)    -> ColorSpace    { self.dst }
    pub fn matrix(&self) -> &Matrix3x3   { &self.matrix }
    /// The source-gamma→linear LUT for u8 (256 entries).
    pub fn decode_u8_lut(&self) -> &[f32] { &self.decode_u8 }
    /// The linear→destination-gamma LUT (4096 entries, used with lerp).
    pub fn encode_lut(&self) -> &[f32] { &self.encode }

    // -----------------------------------------------------------------------
    // Single-pixel / single-value
    // -----------------------------------------------------------------------

    /// Decode src gamma + apply matrix → dst-linear RGB.
    /// Does NOT encode dst transfer function. Efficient when dst is linear (e.g., ACEScg).
    #[inline]
    pub fn decode_to_linear(&self, rgb: [f32; 3]) -> [f32; 3] {
        let tf = self.src.transfer();
        self.matrix.mul_vec([tf.decode(rgb[0]), tf.decode(rgb[1]), tf.decode(rgb[2])])
    }

    /// Full round-trip: decode src → matrix → encode dst.
    #[inline]
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let linear = self.decode_to_linear(rgb);
        let tf = self.dst.transfer();
        [tf.encode(linear[0]), tf.encode(linear[1]), tf.encode(linear[2])]
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

    /// Fast: decode 3 u16 values via LUT → dst-linear RGB.
    #[inline]
    pub fn decode_u16_to_linear(&self, r: u16, g: u16, b: u16) -> [f32; 3] {
        self.matrix.mul_vec([
            self.decode_u16[r as usize],
            self.decode_u16[g as usize],
            self.decode_u16[b as usize],
        ])
    }

    /// Encode one linear value to dst encoding via LUT (with linear interpolation).
    /// `y` is clamped to `[0, 1]` before lookup.
    #[inline]
    pub fn encode_fast(&self, y: f32) -> f32 {
        lookup_encode(y.clamp(0.0, 1.0), &self.encode)
    }

    // -----------------------------------------------------------------------
    // Slice conversion
    // -----------------------------------------------------------------------

    /// In-place full conversion of a flat `f32` RGB(A) slice.
    /// `channels` must be 3 or 4; alpha passes through unchanged.
    pub fn convert_slice(&self, pixels: &mut [f32], channels: usize) -> Result<(), Error> {
        if channels != 3 && channels != 4 {
            return Err(Error::invalid_param(format!("channels must be 3 or 4, got {channels}")));
        }
        for px in pixels.chunks_exact_mut(channels) {
            let [r, g, b] = self.apply([px[0], px[1], px[2]]);
            px[0] = r; px[1] = g; px[2] = b;
        }
        Ok(())
    }

}

// ---------------------------------------------------------------------------
// LUT builders (heap-allocated, no stack overflow risk)
// ---------------------------------------------------------------------------

fn build_decode_u8(tf: TransferFn) -> Box<[f32]> {
    (0u16..256).map(|i| tf.decode(i as f32 / 255.0)).collect::<Vec<_>>().into_boxed_slice()
}

fn build_decode_u16(tf: TransferFn) -> Box<[f32]> {
    (0u32..65536).map(|i| tf.decode(i as f32 / 65535.0)).collect::<Vec<_>>().into_boxed_slice()
}

fn build_encode(tf: TransferFn) -> Box<[f32]> {
    (0u16..4096).map(|i| tf.encode(i as f32 / 4095.0)).collect::<Vec<_>>().into_boxed_slice()
}

#[inline(always)]
fn lookup_encode(y: f32, lut: &[f32]) -> f32 {
    let idx = y.clamp(0.0, 1.0) * (lut.len() - 1) as f32;
    let i = idx as usize;
    let frac = idx - i as f32;
    if i + 1 < lut.len() { lut[i] + frac * (lut[i + 1] - lut[i]) } else { lut[i] }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn color_space_constants() {
        assert_eq!(ColorSpace::SRGB.primaries(),   RgbPrimaries::Bt709);
        assert_eq!(ColorSpace::SRGB.white_point(), WhitePoint::D65);
        assert_eq!(ColorSpace::SRGB.transfer(),    TransferFn::SrgbGamma);
        assert!(ColorSpace::ACES_CG.is_linear());
    }

    #[test]
    fn decode_u8_consistency() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let fast = conv.decode_u8_to_linear(128, 64, 200);
        let slow = conv.decode_to_linear([128.0 / 255.0, 64.0 / 255.0, 200.0 / 255.0]);
        for i in 0..3 {
            assert_approx_eq!(fast[i], slow[i], 1e-5);
        }
    }

    #[test]
    fn decode_u16_consistency() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let fast = conv.decode_u16_to_linear(32768, 16384, 51200);
        let slow = conv.decode_to_linear([32768.0 / 65535.0, 16384.0 / 65535.0, 51200.0 / 65535.0]);
        for i in 0..3 {
            assert_approx_eq!(fast[i], slow[i], 1e-4);
        }
    }

    #[test]
    fn convert_slice_same_space() {
        let conv = ColorSpace::SRGB.converter_to(ColorSpace::SRGB).unwrap();
        let mut pixels = [0.5_f32, 0.3, 0.8, 1.0];
        conv.convert_slice(&mut pixels, 4).unwrap();
        // Same space → no change in color values.
        assert_approx_eq!(pixels[0], 0.5, 1e-4);
        assert_approx_eq!(pixels[3], 1.0, 1e-6);
    }

    #[test]
    fn arbitrary_colorspace_dci_p3() {
        // DCI-P3 uses Gamma26; verifies on-demand LUT generation works.
        let conv = ColorSpace::DCI_P3.converter_to(ColorSpace::ACES_CG).unwrap();
        let result = conv.decode_u8_to_linear(128, 128, 128);
        assert!(result.iter().all(|v| v.is_finite() && *v >= 0.0));
    }

    #[test]
    fn converter_dropped_frees_luts() {
        // Construct and drop several converters to ensure no panics or leaks.
        for _ in 0..10 {
            let _ = ColorSpace::PROPHOTO.converter_to(ColorSpace::ACES_CG).unwrap();
        }
    }
}
