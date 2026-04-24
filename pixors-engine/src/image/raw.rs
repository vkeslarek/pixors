//! Raw image type alias for backward compatibility.
//!
//! `RawImage` is now an alias for `ImageBuffer` (stride-based buffer descriptor).
//! Use `ImageBuffer` directly for new code.

pub use super::buffer::ImageBuffer as RawImage;
