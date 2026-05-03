use crate::container::meta::PixelMeta;

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub meta: PixelMeta,
}

impl Image {
    pub fn new(width: u32, height: u32, meta: PixelMeta) -> Self {
        Self {
            width,
            height,
            meta,
        }
    }
}
