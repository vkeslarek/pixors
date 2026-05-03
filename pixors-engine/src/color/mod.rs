//! Color science types and conversions.

mod chromaticity;
pub mod conversion;
pub mod detect;
pub mod matrix;
mod primaries;
mod space;
mod transfer;

pub use chromaticity::Chromaticity;
pub use conversion::{ColorConversion, lookup_encode};
pub use matrix::{Matrix3x3, bradford_cat, rgb_to_rgb_transform, rgb_to_xyz_matrix};
pub use primaries::{RgbPrimaries, WhitePoint};
pub use space::ColorSpace;
pub use transfer::TransferFn;
