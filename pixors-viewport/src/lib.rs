//! `pixors-viewport` — hardware-accelerated image viewport compiled to WebAssembly.
//!
//! Entry point for the browser: JavaScript imports [`PixorsViewport`] via wasm-bindgen.
//!
//! ## Module layout
//! - [`camera`]   — UV-space camera (pan / zoom / fit), no GPU dependency
//! - [`pipeline`] — wgpu resource factories (bind group layout, render pipeline, sampler)
//! - [`viewport`] — [`PixorsViewport`] wasm-bindgen struct and all public JS-facing methods

mod camera;
mod pipeline;
mod viewport;

pub use viewport::PixorsViewport;
