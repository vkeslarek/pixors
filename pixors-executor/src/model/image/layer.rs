use crate::model::image::buffer::{BufferDesc, ImageBuffer};
use crate::model::image::desc::{BlendMode, Orientation};

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
            name: name.into(),
            buffer,
            offset: (0, 0),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            orientation: Orientation::Identity,
        }
    }
}
