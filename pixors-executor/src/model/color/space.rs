//! Color space definition — primaries + white point + transfer function.

use super::conversion::ColorConversion;
use super::matrix::Matrix3x3;
use super::primaries::{RgbPrimaries, WhitePoint};
use super::transfer::TransferFn;
use crate::error::Error;

/// A color space: primaries + white point + transfer function.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

    pub fn with_optional_params(
        primaries: Option<RgbPrimaries>,
        white_point: Option<WhitePoint>,
        transfer: Option<TransferFn>,
    ) -> Self {
        let p = primaries.unwrap_or(RgbPrimaries::Bt709);
        let wp = white_point.unwrap_or(WhitePoint::D65);
        let t = transfer.unwrap_or(TransferFn::SrgbGamma);
        Self::new(p, wp, t)
    }

    pub fn matrix_to(self, dst: ColorSpace) -> Result<Matrix3x3, Error> {
        if self.primaries == dst.primaries && self.white_point == dst.white_point {
            return Ok(Matrix3x3::IDENTITY);
        }
        crate::model::color::matrix::rgb_to_rgb_transform(
            self.primaries,
            self.white_point,
            dst.primaries,
            dst.white_point,
        )
    }

    pub fn converter_to(self, dst: ColorSpace) -> Result<ColorConversion, Error> {
        ColorConversion::new(self, dst)
    }

    // --- Predefined color spaces ---
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
    pub const ACES_CG: Self = Self::linear(RgbPrimaries::Ap1, WhitePoint::D60);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_space_constants() {
        assert_eq!(ColorSpace::SRGB.primaries(), RgbPrimaries::Bt709);
        assert_eq!(ColorSpace::SRGB.white_point(), WhitePoint::D65);
        assert_eq!(ColorSpace::SRGB.transfer(), TransferFn::SrgbGamma);
        assert!(ColorSpace::ACES_CG.is_linear());
    }
}
