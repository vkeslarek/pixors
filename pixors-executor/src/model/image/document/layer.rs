use serde::{Deserialize, Serialize};

use crate::model::image::ImageBuffer;
use crate::model::image::buffer::BufferDesc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default] Identity,
    FlipH, Rotate180, FlipV, Transpose, Rotate90, Transverse, Rotate270,
}

use std::hash::Hash;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default] Normal,
}

/// Lightweight layer metadata (no pixel data).
pub struct LayerMetadata {
    pub desc: BufferDesc,
    pub orientation: Orientation,
    pub offset: (i32, i32),
    pub name: String,
}

pub struct Layer {
    pub name: String,
    pub buffer: ImageBuffer,
    pub offset: (i32, i32),
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    pub orientation: Orientation,
}

impl Layer {
    pub fn from_buffer(name: impl Into<String>, buffer: ImageBuffer) -> Self {
        Self {
            name: name.into(), buffer, offset: (0, 0), opacity: 1.0,
            blend_mode: BlendMode::Normal, visible: true, orientation: Orientation::Identity,
        }
    }
}
