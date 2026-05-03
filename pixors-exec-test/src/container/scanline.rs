use crate::container::meta::PixelMeta;
use crate::gpu::Buffer;

#[derive(Debug, Clone, Copy)]
pub struct ScanLineCoord {
    pub width: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
pub struct ScanLine {
    pub y: u32,
    pub width: u32,
    pub meta: PixelMeta,
    pub data: Buffer,
}

impl ScanLine {
    pub fn new(y: u32, width: u32, meta: PixelMeta, data: Buffer) -> Self {
        Self {
            y,
            width,
            meta,
            data,
        }
    }
}
