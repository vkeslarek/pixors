//! Color space definition and conversions.

use super::{
    matrix, primaries::{RgbPrimaries, WhitePoint},
    transfer::TransferFn,
    Matrix3x3,
};
use crate::Error;

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
            matrix,
            decode_u8: build_decode_u8(src.transfer()),
            encode: build_encode(dst.transfer()),
        })
    }

    pub fn src(&self) -> ColorSpace {
        self.src
    }
    pub fn matrix(&self) -> &Matrix3x3 {
        &self.matrix
    }

    // -----------------------------------------------------------------------
    // Single-pixel / single-value
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
fn lookup_encode(y: f32, lut: &[f32]) -> f32 {
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
    use crate::assert_approx_eq;

    #[test]
    fn color_space_constants() {
        assert_eq!(ColorSpace::SRGB.primaries(), RgbPrimaries::Bt709);
        assert_eq!(ColorSpace::SRGB.white_point(), WhitePoint::D65);
        assert_eq!(ColorSpace::SRGB.transfer(), TransferFn::SrgbGamma);
        assert!(ColorSpace::ACES_CG.is_linear());
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
}
