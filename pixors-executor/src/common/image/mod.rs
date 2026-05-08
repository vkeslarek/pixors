//! Image model — descriptors, codecs, I/O format implementations.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::common::color::space::ColorSpace;
use crate::common::pixel::AlphaPolicy;
use crate::error::Error;

pub mod codec;
pub mod exif;
pub mod png;
pub mod tiff;

use codec::{ImageDecoder, PageStream};
pub use exif::Metadata;

// ── Descriptors ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dpi {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PixelOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Source,
    Over,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DisposeOp {
    #[default]
    None,
    Background,
    Previous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Identity,
    FlipH,
    Rotate180,
    FlipV,
    Transpose,
    Rotate90,
    Transverse,
    Rotate270,
}

#[derive(Debug, Clone)]
pub struct PageInfo {
    pub name: String,
    pub color_space: ColorSpace,
    pub alpha_policy: AlphaPolicy,
    pub offset: PixelOffset,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    pub orientation: Orientation,
    /// Animation: display time in milliseconds (0 = static).
    pub delay_ms: u32,
    /// Animation: what to do with the previous frame.
    pub dispose: DisposeOp,
}

#[derive(Debug, Clone)]
pub struct ImageDescriptor {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_space: ColorSpace,
    pub dpi: Option<Dpi>,
    pub metadata: Vec<Metadata>,
    pub icc_profile: Option<Vec<u8>>,
    pub pages: Vec<PageInfo>,
}

// ── Alpha mode ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Straight,
    Premultiplied,
    Opaque,
}

impl AlphaMode {
    pub fn has_alpha(self) -> bool {
        matches!(self, AlphaMode::Straight | AlphaMode::Premultiplied)
    }

    pub fn is_straight(self) -> bool {
        matches!(self, AlphaMode::Straight)
    }

    pub fn is_premultiplied(self) -> bool {
        matches!(self, AlphaMode::Premultiplied)
    }

    pub fn is_opaque(self) -> bool {
        matches!(self, AlphaMode::Opaque)
    }
}

// ── Image ────────────────────────────────────────────────────────────────────

pub struct Image {
    pub desc: ImageDescriptor,
    decoder: Arc<dyn ImageDecoder>,
    path: PathBuf,
}

impl Image {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        if let Ok(desc) = png::PngDecoder.decode(path) {
            return Ok(Self {
                desc,
                decoder: Arc::new(png::PngDecoder),
                path: path.to_path_buf(),
            });
        }
        if let Ok(desc) = tiff::TiffDecoder.decode(path) {
            return Ok(Self {
                desc,
                decoder: Arc::new(tiff::TiffDecoder),
                path: path.to_path_buf(),
            });
        }
        Err(Error::internal("unsupported image format"))
    }

    pub fn open_page(&self, page: usize) -> Result<Box<dyn PageStream>, Error> {
        self.decoder.open_stream(&self.path, page)
    }

    pub fn page_count(&self) -> usize {
        self.desc.pages.len()
    }
}

impl Clone for Image {
    fn clone(&self) -> Self {
        Self {
            desc: self.desc.clone(),
            decoder: Arc::clone(&self.decoder),
            path: self.path.clone(),
        }
    }
}
