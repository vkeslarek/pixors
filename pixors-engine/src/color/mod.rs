//! Color science types and conversions.

mod chromaticity;
mod transfer;
mod primaries;
mod conversion;
pub mod detect;
pub mod pipeline;

pub use chromaticity::Chromaticity;
pub use transfer::TransferFn;
pub use primaries::{RgbPrimaries, WhitePoint};
pub use conversion::ColorSpace;
