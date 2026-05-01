use crate::container::meta::PixelMeta;
use crate::container::Container;

#[derive(Debug, Clone, Copy)]
pub struct ScanLineCoord {
    pub width: u32,
    pub y: u32,
}

impl ScanLineCoord {
    pub fn new(width: u32, y: u32) -> Self {
        Self { width, y }
    }
}

#[derive(Debug, Clone)]
pub struct ScanLine {
    pub coord: ScanLineCoord,
    pub meta: PixelMeta,
}

impl ScanLine {
    pub fn new(coord: ScanLineCoord, meta: PixelMeta) -> Self {
        Self { coord, meta }
    }
}

impl Container for ScanLine {
    fn meta(&self) -> &PixelMeta {
        &self.meta
    }
}
