//! Pixel types and traits.

mod component;
mod types;
mod pixel;

pub use component::Component;
pub use types::{Rgba, Rgb, Gray, GrayAlpha};
pub use pixel::{Pixel, PixelLayout};