use crate::common::pixel::meta::PixelMeta;
use crate::data::buffer::Buffer;

pub const DEFAULT_TILE_SIZE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileGridPos {
    pub mip_level: u32,
    pub tx: u32,
    pub ty: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub mip_level: u32,
    pub tx: u32,
    pub ty: u32,
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
    /// Nominal tile size (e.g. 256). Edge tiles are smaller but this is always the full size.
    pub tile_size: u32,
    /// Image width at this mip level.
    pub image_width: u32,
    /// Image height at this mip level.
    pub image_height: u32,
}

impl TileCoord {
    /// `image_width` and `image_height` must be mip-0 values.
    /// The constructor computes mip-adjusted pixel dimensions internally.
    pub fn new(
        mip_level: u32,
        tx: u32,
        ty: u32,
        tile_size: u32,
        image_width: u32,
        image_height: u32,
    ) -> Self {
        let mip_w = (image_width >> mip_level).max(1);
        let mip_h = (image_height >> mip_level).max(1);
        let px = tx * tile_size;
        let py = ty * tile_size;
        let width = if px >= mip_w {
            0
        } else {
            (mip_w - px).min(tile_size)
        };
        let height = if py >= mip_h {
            0
        } else {
            (mip_h - py).min(tile_size)
        };
        Self {
            mip_level,
            tx,
            ty,
            px,
            py,
            width,
            height,
            tile_size,
            image_width,
            image_height,
        }
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
