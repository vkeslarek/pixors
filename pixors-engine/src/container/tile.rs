use crate::container::meta::PixelMeta;
use crate::storage::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub tx: u32,
    pub ty: u32,
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
}

impl TileCoord {
    pub fn new(tx: u32, ty: u32, tile_size: u32, image_width: u32, image_height: u32) -> Self {
        let px = tx * tile_size;
        let py = ty * tile_size;
        let width = if px >= image_width {
            0
        } else {
            (image_width - px).min(tile_size)
        };
        let height = if py >= image_height {
            0
        } else {
            (image_height - py).min(tile_size)
        };
        Self {
            tx,
            ty,
            px,
            py,
            width,
            height,
        }
    }

    pub fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }

    pub fn bounds(&self) -> (u32, u32, u32, u32) {
        (self.px, self.py, self.width, self.height)
    }
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub coord: TileCoord,
    pub meta: PixelMeta,
    pub data: Buffer,
}

impl Tile {
    pub fn new(coord: TileCoord, meta: PixelMeta, data: Buffer) -> Self {
        Self { coord, meta, data }
    }
}
