use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::color::space::ColorSpace;
use crate::model::image::buffer::BufferDescriptor;

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
    pub buffer_desc: BufferDescriptor,
    pub offset: PixelOffset,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    pub orientation: Orientation,
}

#[derive(Debug, Clone)]
pub struct ImageDescriptor {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_space: ColorSpace,
    pub dpi: Option<Dpi>,
    pub exif_tags: HashMap<String, String>,
    pub icc_profile: Option<Vec<u8>>,
    pub pages: Vec<PageInfo>,
}
