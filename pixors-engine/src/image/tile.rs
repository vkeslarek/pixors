use crate::pixel::Rgba;
use crate::convert::ColorConversion;
use crate::pixel::AlphaPolicy;
use half::f16;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Identity of a tile — everything the engine needs to locate any tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileCoord {
    pub mip_level: u32,
    pub tx: u32,
    pub ty: u32,
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
}

impl TileCoord {
    pub fn new(
        mip_level: u32,
        tx: u32,
        ty: u32,
        tile_size: u32,
        image_width: u32,
        image_height: u32,
    ) -> Self {
        let px = tx * tile_size;
        let py = ty * tile_size;
        let width = if px >= image_width { 0 } else { (image_width - px).min(tile_size) };
        let height = if py >= image_height { 0 } else { (image_height - py).min(tile_size) };
        Self { mip_level, tx, ty, px, py, width, height }
    }

    pub fn from_xywh(mip_level: u32, px: u32, py: u32, width: u32, height: u32) -> Self {
        Self { mip_level, tx: 0, ty: 0, px, py, width, height }
    }

    pub fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }

    pub fn bounds(&self) -> (u32, u32, u32, u32) {
        (self.px, self.py, self.width, self.height)
    }
}

/// Tile with data in memory — generic over pixel type.
/// `P = Rgba<f16>` for ACEScg storage.
/// `P = u8` for display sRGB (raw RGBA8).
#[derive(Clone)]
pub struct Tile<P: Clone> {
    pub coord: TileCoord,
    pub data: Arc<Vec<P>>,
}

impl<P: Clone> Tile<P> {
    pub fn new(coord: TileCoord, data: Vec<P>) -> Self {
        Self { coord, data: Arc::new(data) }
    }
}

impl Tile<Rgba<f16>> {
    pub fn to_srgb_u8(&self, conv: &ColorConversion) -> Tile<u8> {
        let pixels: Vec<[u8; 4]> =
            conv.convert_pixels::<Rgba<f16>, [u8; 4]>(&self.data, AlphaPolicy::Straight);
        let bytes: Vec<u8> = bytemuck::cast_slice::<[u8; 4], u8>(pixels.as_slice()).to_vec();
        Tile { coord: self.coord, data: Arc::new(bytes) }
    }
}

/// A grid of tile coordinates covering an entire image at one MIP level.
#[derive(Clone)]
pub struct TileGrid {
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
    tiles: Vec<TileCoord>,
}

impl std::fmt::Debug for TileGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileGrid")
            .field("width", &self.image_width)
            .field("height", &self.image_height)
            .field("tiles_count", &self.tiles.len())
            .field("tile_size", &self.tile_size)
            .finish()
    }
}

impl TileGrid {
    pub fn new(width: u32, height: u32, tile_size: u32) -> Self {
        assert!(tile_size > 0, "Tile size must be positive");
        let tiles_x = width.div_ceil(tile_size);
        let tiles_y = height.div_ceil(tile_size);
        let mut tiles = Vec::with_capacity((tiles_x * tiles_y) as usize);
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                tiles.push(TileCoord::new(0, tx, ty, tile_size, width, height));
            }
        }
        Self { image_width: width, image_height: height, tile_size, tiles }
    }

    pub fn width(&self) -> u32 { self.image_width }
    pub fn height(&self) -> u32 { self.image_height }
    pub fn tile_size(&self) -> u32 { self.tile_size }
    pub fn tile_count(&self) -> usize { self.tiles.len() }

    pub fn tiles(&self) -> impl Iterator<Item = &TileCoord> {
        self.tiles.iter()
    }

    pub fn tile(&self, index: usize) -> Option<&TileCoord> {
        self.tiles.get(index)
    }

    pub fn tile_at(&self, tx: u32, ty: u32) -> Option<&TileCoord> {
        self.tiles.iter().find(|t| t.tx == tx && t.ty == ty)
    }

    /// Returns tiles that intersect with the given viewport rectangle.
    /// Viewport coordinates are in image space (pixels).
    pub fn tiles_in_viewport(
        &self,
        mip_level: u32,
        viewport_x: f32,
        viewport_y: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Vec<TileCoord> {
        let vx_min = viewport_x.floor() as i64;
        let vy_min = viewport_y.floor() as i64;
        let vx_max = (viewport_x + viewport_width).ceil() as i64;
        let vy_max = (viewport_y + viewport_height).ceil() as i64;

        self.tiles.iter()
            .filter(|t| {
                let t_min_x = t.px as i64;
                let t_min_y = t.py as i64;
                let t_max_x = t_min_x + t.width as i64;
                let t_max_y = t_min_y + t.height as i64;
                t_max_x > vx_min && t_min_x < vx_max &&
                t_max_y > vy_min && t_min_y < vy_max
            })
            .map(|t| TileCoord {
                mip_level,
                ..*t
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorSpace;

    #[test]
    fn test_tile_coord_creation() {
        let c = TileCoord::new(0, 1, 2, 256, 1000, 800);
        assert_eq!(c.mip_level, 0);
        assert_eq!(c.tx, 1);
        assert_eq!(c.ty, 2);
        assert_eq!(c.px, 256);
        assert_eq!(c.py, 512);
        assert_eq!(c.width, 256);
        assert_eq!(c.height, 256);
        assert_eq!(c.pixel_count(), 256 * 256);
    }

    #[test]
    fn test_tile_coord_edge_tile() {
        let c = TileCoord::new(0, 3, 3, 256, 1000, 800);
        assert_eq!(c.px, 768);
        assert_eq!(c.py, 768);
        assert_eq!(c.width, 232);  // 1000 - 768
        assert_eq!(c.height, 32);  // 800 - 768
    }

    #[test]
    fn test_tile_coord_mip_level() {
        let c = TileCoord::new(2, 0, 0, 256, 500, 400);
        assert_eq!(c.mip_level, 2);
    }

    #[test]
    fn test_tile_creation() {
        let coord = TileCoord::new(0, 0, 0, 256, 1000, 800);
        let data = vec![Rgba::new(f16::from_f32(0.5), f16::from_f32(0.3), f16::from_f32(0.2), f16::ONE); 256 * 256];
        let tile: Tile<Rgba<f16>> = Tile::new(coord, data);
        assert_eq!(tile.coord.width, 256);
        assert_eq!(tile.data.len(), 256 * 256);
    }

    #[test]
    fn test_tile_to_srgb_u8() {
        let coord = TileCoord::new(0, 0, 0, 256, 256, 256);
        let data = vec![Rgba::new(f16::from_f32(0.5), f16::from_f32(0.3), f16::from_f32(0.2), f16::ONE)];
        let tile = Tile::new(coord, data);
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        let srgb = tile.to_srgb_u8(&conv);
        assert_eq!(srgb.data.len(), 4); // 1 pixel × 4 channels
    }

    #[test]
    fn test_tile_grid_creation() {
        let grid = TileGrid::new(1000, 800, 256);
        assert_eq!(grid.tile_size(), 256);
        assert_eq!(grid.tile_count(), 4 * 4);
        let first = grid.tile(0).unwrap();
        assert_eq!(first.px, 0);
        assert_eq!(first.py, 0);
        assert_eq!(first.width, 256);
        assert_eq!(first.height, 256);
        let last = grid.tile(grid.tile_count() - 1).unwrap();
        assert_eq!(last.px, 768);
        assert_eq!(last.py, 768);
        assert_eq!(last.width, 232);
        assert_eq!(last.height, 32);
    }

    #[test]
    fn test_tiles_in_viewport() {
        let grid = TileGrid::new(1000, 800, 256);
        let tiles = grid.tiles_in_viewport(0, 0.0, 0.0, 300.0, 300.0);
        assert_eq!(tiles.len(), 4);
        let all = grid.tiles_in_viewport(0, 0.0, 0.0, 1000.0, 800.0);
        assert_eq!(all.len(), grid.tile_count());
        let none = grid.tiles_in_viewport(0, 2000.0, 2000.0, 100.0, 100.0);
        assert_eq!(none.len(), 0);
    }

    #[test]
    fn test_tile_coord_bounds() {
        let c = TileCoord::new(1, 2, 3, 128, 512, 512);
        assert_eq!(c.bounds(), (256, 384, 128, 128));
    }
}
