//! Image / Layer model — top-level image abstraction.
//!
//! `Image` is what you get from loading any file: layers + metadata.
//! PNG = 1 layer, TIFF multi-page = N layers.

mod layer;
pub use layer::{BlendMode, Layer, LayerMetadata, Orientation};

use crate::model::image::ImageBuffer;
use std::collections::HashMap;

#[derive(Default, Debug, Clone)]
pub struct ImageMetadata {
    pub source_format: Option<String>,
    pub source_path: Option<std::path::PathBuf>,
    pub dpi: Option<(f32, f32)>,
    pub text: HashMap<String, String>,
    pub raw_icc: Option<Vec<u8>>,
}

pub struct ImageInfo {
    pub layer_count: usize,
    pub metadata: ImageMetadata,
}

pub struct Image {
    pub layers: Vec<Layer>,
    pub metadata: ImageMetadata,
}

impl Image {
    pub fn single_layer(name: impl Into<String>, buffer: ImageBuffer) -> Self {
        Self {
            layers: vec![Layer::from_buffer(name, buffer)],
            metadata: ImageMetadata::default(),
        }
    }
}
