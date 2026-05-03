use crate::container::meta::PixelMeta;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Layer {
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub opacity: f32,
    pub meta: PixelMeta,
}

impl Layer {
    pub fn new(index: u32, width: u32, height: u32, meta: PixelMeta) -> Self {
        Self { index, width, height, x: 0, y: 0, opacity: 1.0, meta }
    }
}

#[derive(Debug, Clone)]
pub struct Layers {
    pub layers: Arc<Vec<Layer>>,
}

impl Layers {
    pub fn new(layers: Vec<Layer>) -> Self {
        Self { layers: Arc::new(layers) }
    }
}
