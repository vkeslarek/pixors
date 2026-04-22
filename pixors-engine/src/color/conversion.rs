//! Color space definition and conversion pipeline.

use super::{
    matrix,
    primaries::{RgbPrimaries, WhitePoint},
    transfer::TransferFn,
};
use crate::Error;

/// A color space defined by primaries, white point, and transfer function.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ColorSpace {
    primaries: RgbPrimaries,
    white_point: WhitePoint,
    transfer: TransferFn,
}

impl ColorSpace {
    /// Creates a new color space.
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

    /// Creates a linear color space (transfer function = `Linear`).
    pub const fn linear(primaries: RgbPrimaries, white_point: WhitePoint) -> Self {
        Self {
            primaries,
            white_point,
            transfer: TransferFn::Linear,
        }
    }

    /// Returns the primaries.
    pub const fn primaries(&self) -> RgbPrimaries {
        self.primaries
    }

    /// Returns the white point.
    pub const fn white_point(&self) -> WhitePoint {
        self.white_point
    }

    /// Returns the transfer function.
    pub const fn transfer(&self) -> TransferFn {
        self.transfer
    }

    /// Whether the color space is linear (no gamma).
    pub const fn is_linear(&self) -> bool {
        self.transfer.is_linear()
    }

    /// Returns a copy of this color space with linear transfer function.
    pub const fn as_linear(&self) -> Self {
        Self {
            primaries: self.primaries,
            white_point: self.white_point,
            transfer: TransferFn::Linear,
        }
    }

    /// Returns a copy with a different transfer function.
    pub const fn with_transfer(&self, transfer: TransferFn) -> Self {
        Self {
            primaries: self.primaries,
            white_point: self.white_point,
            transfer,
        }
    }

    /// Returns a copy with different primaries.
    pub const fn with_primaries(&self, primaries: RgbPrimaries) -> Self {
        Self {
            primaries,
            white_point: self.white_point,
            transfer: self.transfer,
        }
    }

    /// Returns a copy with a different white point.
    pub const fn with_white_point(&self, white_point: WhitePoint) -> Self {
        Self {
            primaries: self.primaries,
            white_point,
            transfer: self.transfer,
        }
    }

    /// Returns the transformation matrix from this color space to another
    /// (linear RGB → linear RGB, excluding transfer functions).
    ///
    /// This matrix should be applied to linear RGB values after decoding
    /// the source transfer function and before encoding the destination
    /// transfer function.
    pub fn matrix_to(&self, dst: ColorSpace) -> Result<matrix::Matrix3x3, Error> {
        if self.primaries == dst.primaries && self.white_point == dst.white_point {
            Ok(matrix::Matrix3x3::IDENTITY)
        } else {
            matrix::rgb_to_rgb_transform(
                self.primaries,
                self.white_point,
                dst.primaries,
                dst.white_point,
            )
        }
    }

    // -------------------------------------------------------------------------
    // Predefined color spaces (matching Phase1.md and kolor)
    // -------------------------------------------------------------------------

    /// sRGB (BT.709 primaries, D65, sRGB gamma).
    pub const SRGB: Self = Self::new(
        RgbPrimaries::Bt709,
        WhitePoint::D65,
        TransferFn::SrgbGamma,
    );

    /// Linear sRGB (BT.709 primaries, D65, linear).
    pub const LINEAR_SRGB: Self = Self::linear(RgbPrimaries::Bt709, WhitePoint::D65);

    /// Rec.709 (BT.709 primaries, D65, Rec.709 gamma).
    pub const REC709: Self = Self::new(
        RgbPrimaries::Bt709,
        WhitePoint::D65,
        TransferFn::Rec709Gamma,
    );

    /// Rec.2020 (BT.2020 primaries, D65, Rec.709 gamma).
    pub const REC2020: Self = Self::new(
        RgbPrimaries::Bt2020,
        WhitePoint::D65,
        TransferFn::Rec709Gamma,
    );

    /// Linear Rec.2020 (BT.2020 primaries, D65, linear).
    pub const LINEAR_REC2020: Self = Self::linear(RgbPrimaries::Bt2020, WhitePoint::D65);

    /// Adobe RGB (Adobe 1998 primaries, D65, gamma 2.2).
    pub const ADOBE_RGB: Self = Self::new(
        RgbPrimaries::Adobe1998,
        WhitePoint::D65,
        TransferFn::Gamma22,
    );

    /// Display‑P3 (P3 primaries, D65, sRGB gamma).
    pub const DISPLAY_P3: Self = Self::new(
        RgbPrimaries::P3,
        WhitePoint::D65,
        TransferFn::SrgbGamma,
    );

    /// Linear Display‑P3 (P3 primaries, D65, linear).
    pub const LINEAR_DISPLAY_P3: Self = Self::linear(RgbPrimaries::P3, WhitePoint::D65);

    /// DCI-P3 Theater (P3 primaries, P3-DCI white point, gamma 2.6).
    pub const DCI_P3: Self = Self::new(
        RgbPrimaries::P3,
        WhitePoint::P3Dci,
        TransferFn::Gamma26,
    );

    /// ProPhoto RGB (ProPhoto primaries, D50, ProPhoto gamma).
    pub const PROPHOTO: Self = Self::new(
        RgbPrimaries::ProPhoto,
        WhitePoint::D50,
        TransferFn::ProPhotoGamma,
    );

    /// ACES2065‑1 (AP0 primaries, D60, linear).
    pub const ACES2065_1: Self = Self::linear(RgbPrimaries::Ap0, WhitePoint::D60);

    /// ACEScg (AP1 primaries, D60, linear) – the working space.
    pub const ACES_CG: Self = Self::linear(RgbPrimaries::Ap1, WhitePoint::D60);
}

// ---------------------------------------------------------------------------
// Pixel conversion (general-purpose, used by tests and future API surface)
// ---------------------------------------------------------------------------

/// In-place color space conversion for a flat `f32` RGB(A) pixel slice.
/// `channels_per_pixel` must be 3 or 4; alpha (index 3) passes through unchanged.
#[allow(dead_code)]
pub fn convert_pixels(
    pixels: &mut [f32],
    channels_per_pixel: u8,
    src: ColorSpace,
    dst: ColorSpace,
) -> Result<(), Error> {
    if channels_per_pixel != 3 && channels_per_pixel != 4 {
        return Err(Error::invalid_param(format!(
            "channels_per_pixel must be 3 or 4, got {}",
            channels_per_pixel
        )));
    }
    if pixels.len() % channels_per_pixel as usize != 0 {
        return Err(Error::invalid_param(format!(
            "pixels length {} not multiple of {}",
            pixels.len(),
            channels_per_pixel
        )));
    }

    let ch = channels_per_pixel as usize;

    // If same color space, nothing to do.
    if src == dst {
        return Ok(());
    }

    // Pre‑compute transformation matrix (linear RGB → linear RGB).
    let transform = if src.primaries() == dst.primaries() && src.white_point() == dst.white_point() {
        // No primaries/white point change needed.
        None
    } else {
        Some(matrix::rgb_to_rgb_transform(
            src.primaries(),
            src.white_point(),
            dst.primaries(),
            dst.white_point(),
        )?)
    };

    // Process pixels in chunks.
    for chunk in pixels.chunks_mut(ch) {
        let (r, g, b) = (chunk[0], chunk[1], chunk[2]);

        // 1. Decode source transfer
        let r_lin = src.transfer().decode(r);
        let g_lin = src.transfer().decode(g);
        let b_lin = src.transfer().decode(b);

        // 2. Apply primaries + white point conversion (if needed)
        let (r_conv, g_conv, b_conv) = if let Some(ref mat) = transform {
            let rgb = mat.mul_vec([r_lin, g_lin, b_lin]);
            (rgb[0], rgb[1], rgb[2])
        } else {
            (r_lin, g_lin, b_lin)
        };

        // 3. Encode with destination transfer
        chunk[0] = dst.transfer().encode(r_conv);
        chunk[1] = dst.transfer().encode(g_conv);
        chunk[2] = dst.transfer().encode(b_conv);
        // chunk[3] (alpha) stays unchanged.
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn color_space_constants() {
        assert_eq!(ColorSpace::SRGB.primaries(), RgbPrimaries::Bt709);
        assert_eq!(ColorSpace::SRGB.white_point(), WhitePoint::D65);
        assert_eq!(ColorSpace::SRGB.transfer(), TransferFn::SrgbGamma);

        assert_eq!(ColorSpace::ACES_CG.primaries(), RgbPrimaries::Ap1);
        assert_eq!(ColorSpace::ACES_CG.white_point(), WhitePoint::D60);
        assert!(ColorSpace::ACES_CG.is_linear());
    }

    #[test]
    fn convert_pixels_same_space() {
        let mut pixels = [0.5, 0.3, 0.8, 1.0];
        convert_pixels(&mut pixels, 4, ColorSpace::SRGB, ColorSpace::SRGB).unwrap();
        assert_eq!(pixels, [0.5, 0.3, 0.8, 1.0]);
    }

    #[test]
    fn convert_pixels_linear_srgb_to_srgb() {
        // Linear sRGB 0.5 → sRGB encoded ≈ 0.735 (middle gray)
        let mut pixels = [0.5, 0.5, 0.5];
        convert_pixels(&mut pixels, 3, ColorSpace::LINEAR_SRGB, ColorSpace::SRGB).unwrap();
        let expected = TransferFn::SrgbGamma.encode(0.5);
        assert_approx_eq!(pixels[0], expected, 1e-5);
        assert_approx_eq!(pixels[1], expected, 1e-5);
        assert_approx_eq!(pixels[2], expected, 1e-5);
    }

    #[test]
    fn convert_pixels_srgb_to_linear_srgb() {
        let mut pixels = [0.735, 0.735, 0.735];
        convert_pixels(&mut pixels, 3, ColorSpace::SRGB, ColorSpace::LINEAR_SRGB).unwrap();
        let expected = TransferFn::SrgbGamma.decode(0.735);
        assert_approx_eq!(pixels[0], expected, 1e-5);
        assert_approx_eq!(pixels[1], expected, 1e-5);
        assert_approx_eq!(pixels[2], expected, 1e-5);
    }

    #[test]
    fn convert_pixels_with_alpha() {
        let mut pixels = [0.5, 0.3, 0.8, 0.9];
        convert_pixels(&mut pixels, 4, ColorSpace::LINEAR_SRGB, ColorSpace::SRGB).unwrap();
        // Alpha unchanged
        assert_approx_eq!(pixels[3], 0.9, 1e-6);
    }

    #[test]
    fn convert_pixels_invalid_channels() {
        let mut pixels = [0.5, 0.3, 0.8];
        // channels_per_pixel = 2 is invalid
        let err = convert_pixels(&mut pixels, 2, ColorSpace::SRGB, ColorSpace::LINEAR_SRGB)
            .unwrap_err();
        assert!(matches!(err, Error::InvalidParameter(_)));
    }

    #[test]
    fn convert_pixels_length_mismatch() {
        let mut pixels = [0.5, 0.3, 0.8, 1.0, 0.2]; // 5 elements, not multiple of 3 or 4
        let err = convert_pixels(&mut pixels, 3, ColorSpace::SRGB, ColorSpace::LINEAR_SRGB)
            .unwrap_err();
        assert!(matches!(err, Error::InvalidParameter(_)));
    }
}