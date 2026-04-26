//! I/O modules for image formats.

use crate::color::ColorSpace;
use crate::error::Error;
use crate::image::{AlphaMode, ImageBuffer};
use std::path::Path;

pub mod png;
pub mod tiff;

// ---------------------------------------------------------------------------
// ImageReader trait — any image format that can be loaded
// ---------------------------------------------------------------------------

/// Format-agnostic image reader. Implemented per format (PNG, TIFF, etc.).
pub trait ImageReader: Send + Sync {
    /// Returns true if this reader believes it can decode the given file.
    fn can_handle(&self, path: &Path) -> bool;

    /// Read image dimensions and color metadata without full decode.
    fn read_metadata(&self, path: &Path) -> Result<(u32, u32, ColorSpace, AlphaMode), Error>;

    /// Load the full image into an ImageBuffer.
    fn load(&self, path: &Path) -> Result<ImageBuffer, Error>;
}

/// All registered image formats, in priority order.
pub fn all_readers() -> &'static [&'static dyn ImageReader] {
    &[&png::PngFormat, &tiff::TiffFormat]
}
