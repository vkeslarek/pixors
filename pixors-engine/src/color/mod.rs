//! Color science types and conversions.

mod chromaticity;
mod transfer;
mod primaries;
mod space;
pub mod detect;
pub mod conversion;
pub mod matrix;

pub use chromaticity::Chromaticity;
pub use transfer::TransferFn;
pub use primaries::{RgbPrimaries, WhitePoint};
pub use space::ColorSpace;
pub use conversion::{ColorConversion, lookup_encode};
pub use matrix::{
    Matrix3x3, bradford_cat, rgb_to_rgb_transform, rgb_to_xyz_matrix,
};
