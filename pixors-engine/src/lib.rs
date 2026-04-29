//! Pixors — image processing pixors-engine (Phase 1: Image I/O abstraction).
//!
//! This library provides color-managed image loading and saving with support for PNG.
//! All internal processing uses ACEScg linear premultiplied RGBA f16 format.

pub mod composite;
pub mod config;
pub mod approx;
pub mod color;
pub mod pixel;
pub mod image;
pub mod stream;
pub mod convert;
pub mod io;
pub mod utils;
pub mod server;
pub mod storage;
pub mod error;
pub mod pipeline;

pub use color::ColorSpace;
pub use image::AlphaMode;
pub use pixel::Rgba;