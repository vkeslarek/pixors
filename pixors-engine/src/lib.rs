//! Pixors — image processing pixors-engine (Phase 1: Image I/O abstraction).
//!
//! This library provides color-managed image loading and saving with support for PNG.
//! All internal processing uses ACEScg linear premultiplied RGBA f16 format.

pub mod approx;
pub mod color;
pub mod pixel;
pub mod image;
pub mod convert;
pub mod io;
pub mod utils;
pub mod viewport;

pub use color::ColorSpace;
pub use image::{AlphaMode, RawImage, TypedImage};
pub use pixel::{Component, Pixel, Rgba, Rgb, Gray, GrayAlpha};
pub use convert::{premultiply, unpremultiply};
pub use io::png;

/// Error type for all library operations.
pub mod error;

/// Re-export of the main error type.
pub use error::Error;