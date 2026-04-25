//! Color science types and conversions.

mod chromaticity;
mod xyz;
mod transfer;
mod primaries;
mod matrix;
mod conversion;

pub use chromaticity::Chromaticity;
pub use xyz::{Xyz, Xyy};
pub use transfer::TransferFn;
pub use primaries::{RgbPrimaries, WhitePoint};
pub use conversion::{ColorSpace, ColorConversion};
pub use matrix::Matrix3x3;

/// Build a ColorSpace from optional components, falling back to sRGB when unknown.
pub fn color_space_from_params(
    primaries: Option<RgbPrimaries>,
    white_point: Option<WhitePoint>,
    transfer: Option<TransferFn>,
) -> ColorSpace {
    let p = primaries.unwrap_or(RgbPrimaries::Bt709);
    let wp = white_point.unwrap_or(WhitePoint::D65);
    let t = transfer.unwrap_or(TransferFn::SrgbGamma);
    ColorSpace::new(p, wp, t)
}

/// Map a gamma value (1/gamma from file metadata) to a transfer function.
pub fn transfer_from_gamma(g: f32) -> Option<TransferFn> {
    if (g - 1.0 / 2.2).abs() < 0.01 {
        Some(TransferFn::Gamma22)
    } else if (g - 1.0 / 2.4).abs() < 0.01 {
        Some(TransferFn::Gamma24)
    } else if (g - 1.0 / 2.2).abs() < 0.05 {
        Some(TransferFn::Gamma22)
    } else {
        None
    }
}
