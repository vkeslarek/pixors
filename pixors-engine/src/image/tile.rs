//! Tile-based image representation for zero-copy tiling.

use crate::error::Error;
use crate::image::TypedImage;
use crate::pixel::Rgba;
use crate::color::{ColorSpace, ColorConversion};
use half::f16;
use std::sync::Arc;

/// A tile representing a rectangular region of an image.
#[derive(Debug, Clone, Copy)]
pub struct Tile {
    /// X coordinate in the original image (pixels).
    pub x: u32,
    /// Y coordinate in the original image (pixels).
    pub y: u32,
    /// Width of the tile (pixels).
    pub width: u32,
    /// Height of the tile (pixels).
    pub height: u32,
}

impl Tile {
    /// Creates a new tile.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the area of the tile in pixels.
    pub fn area(&self) -> usize {
        (self.width * self.height) as usize
    }

    /// Checks if this tile overlaps with another tile.
    pub fn overlaps(&self, other: &Tile) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// Returns the tile's bounds as (x, y, width, height).
    pub fn bounds(&self) -> (u32, u32, u32, u32) {
        (self.x, self.y, self.width, self.height)
    }
}

/// A grid of tiles covering an entire image.
#[derive(Clone)]
pub struct TileGrid {
    /// Image width (pixels).
    width: u32,
    /// Image height (pixels).
    height: u32,
    /// List of tiles covering the image.
    tiles: Vec<Tile>,
    /// Tile size used for retiling (width = height = tile_size).
    tile_size: u32,
    /// Cached color conversion for extremely fast sRGB tile extraction.
    to_srgb: ColorConversion,
    /// Optional reference to the underlying typed image (for backward compatibility).
    typed_image: Option<Arc<TypedImage<Rgba<f16>>>>,
}

impl std::fmt::Debug for TileGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileGrid")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("tiles_count", &self.tiles.len())
            .field("tile_size", &self.tile_size)
            .finish()
    }
}

impl TileGrid {
    /// Creates a tile grid from image dimensions using the specified tile size.
    pub fn new(width: u32, height: u32, tile_size: u32) -> Self {
        let _sw = crate::debug_stopwatch!("tile_grid_new");
        assert!(tile_size > 0, "Tile size must be positive");
        
        // Calculate number of tiles in each dimension
        let tiles_x = (width + tile_size - 1) / tile_size;
        let tiles_y = (height + tile_size - 1) / tile_size;
        
        let mut tiles = Vec::with_capacity((tiles_x * tiles_y) as usize);
        
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let x = tx * tile_size;
                let y = ty * tile_size;
                let tile_width = (width - x).min(tile_size);
                let tile_height = (height - y).min(tile_size);
                
                tiles.push(Tile::new(x, y, tile_width, tile_height));
            }
        }
        
        let to_srgb = ColorSpace::ACES_CG
            .converter_to(ColorSpace::SRGB)
            .expect("ACEScg → sRGB conversion is always valid");
        
        Self {
            width,
            height,
            tiles,
            tile_size,
            to_srgb,
            typed_image: None,
        }
    }

    /// Creates a tile grid from a typed image (legacy compatibility).
    /// This is a zero-copy operation: tiles are just metadata referencing the image.
    pub fn from_typed_image(typed_image: Arc<TypedImage<Rgba<f16>>>, tile_size: u32) -> Self {
        let mut grid = Self::new(typed_image.width, typed_image.height, tile_size);
        grid.typed_image = Some(typed_image);
        grid
    }

    /// Returns the image width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the image height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the tile size used for this grid.
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Returns the number of tiles in the grid.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Returns an iterator over all tiles.
    pub fn tiles(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.iter()
    }

    /// Returns the tile at the given index.
    pub fn tile(&self, index: usize) -> Option<&Tile> {
        self.tiles.get(index)
    }

    /// Converts tile pixel data from ACEScg premul f16 to sRGB u8.
    /// The `data` slice must contain exactly `tile.width * tile.height` pixels.
    pub fn tile_data_to_rgba8(&self, tile: &Tile, data: &[Rgba<f16>]) -> Result<Vec<u8>, Error> {
        let _sw = crate::debug_stopwatch!("tile_data_to_rgba8");
        // Validate tile bounds
        if tile.x + tile.width > self.width || tile.y + tile.height > self.height {
            return Err(Error::invalid_param(format!(
                "Tile bounds ({}, {}, {}, {}) exceed image dimensions ({}x{})",
                tile.x, tile.y, tile.width, tile.height,
                self.width, self.height
            )));
        }
        // Validate data length
        let expected_len = (tile.width * tile.height) as usize;
        if data.len() != expected_len {
            return Err(Error::invalid_param(format!(
                "Tile data length {} does not match tile area {}",
                data.len(), expected_len
            )));
        }

        Ok(crate::convert::convert_acescg_premul_pixels_to_srgb_u8(
            data,
            &self.to_srgb,
        ))
    }

    /// Returns a reference to the underlying typed image, if any.
    pub fn typed_image(&self) -> Option<&Arc<TypedImage<Rgba<f16>>>> {
        self.typed_image.as_ref()
    }

    /// Extracts RGBA8 pixel data for a specific tile (legacy method).
    /// Requires that the TileGrid was created with a typed image.
    pub fn extract_tile_rgba8(&self, tile: &Tile) -> Result<Vec<u8>, Error> {
        let _sw = crate::debug_stopwatch!("extract_tile_rgba8");
        let Some(typed_image) = &self.typed_image else {
            return Err(Error::invalid_param("TileGrid has no underlying typed image"));
        };
        // Validate tile bounds
        if tile.x + tile.width > typed_image.width || tile.y + tile.height > typed_image.height {
            return Err(Error::invalid_param(format!(
                "Tile bounds ({}, {}, {}, {}) exceed image dimensions ({}x{})",
                tile.x, tile.y, tile.width, tile.height,
                typed_image.width, typed_image.height
            )));
        }

        // Extract tile region using optimized region conversion
        Ok(crate::convert::convert_acescg_premul_region_to_srgb_u8(
            typed_image,
            tile.x,
            tile.y,
            tile.width,
            tile.height,
            &self.to_srgb,
        ))
    }

    /// Returns tiles that intersect with the given viewport rectangle.
    /// Viewport coordinates are in image space (pixels).
    pub fn tiles_in_viewport(&self, viewport_x: f32, viewport_y: f32, viewport_width: f32, viewport_height: f32) -> Vec<&Tile> {
        let viewport_min_x = viewport_x.floor() as i64;
        let viewport_min_y = viewport_y.floor() as i64;
        let viewport_max_x = (viewport_x + viewport_width).ceil() as i64;
        let viewport_max_y = (viewport_y + viewport_height).ceil() as i64;
        
        self.tiles.iter()
            .filter(|tile| {
                let tile_min_x = tile.x as i64;
                let tile_min_y = tile.y as i64;
                let tile_max_x = tile_min_x + tile.width as i64;
                let tile_max_y = tile_min_y + tile.height as i64;
                
                tile_max_x > viewport_min_x && tile_min_x < viewport_max_x &&
                tile_max_y > viewport_min_y && tile_min_y < viewport_max_y
            })
            .collect()
    }
}

/// Retiles a typed image into a grid of tiles (zero-copy).
pub fn retile(typed_image: Arc<TypedImage<Rgba<f16>>>, tile_size: u32) -> TileGrid {
    TileGrid::from_typed_image(typed_image, tile_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{ColorSpace, RgbPrimaries, TransferFn, WhitePoint};
    use crate::image::{AlphaMode, ChannelLayoutKind, RawImage, SampleLayout, SampleType};
    
    fn create_test_raw_image(width: u32, height: u32) -> RawImage {
        let pixel_count = (width * height) as usize;
        let channels = 4; // RGBA
        let data = vec![0u8; pixel_count * channels * 2]; // f16 = 2 bytes per sample
        
        RawImage::new(
            width,
            height,
            SampleType::F16,
            ChannelLayoutKind::Rgba,
            SampleLayout::Interleaved,
            ColorSpace::new(RgbPrimaries::Bt709, WhitePoint::D65, TransferFn::SrgbGamma),
            AlphaMode::Straight,
            data,
        ).unwrap()
    }
    
    #[test]
    fn test_tile_creation() {
        let tile = Tile::new(10, 20, 256, 256);
        assert_eq!(tile.x, 10);
        assert_eq!(tile.y, 20);
        assert_eq!(tile.width, 256);
        assert_eq!(tile.height, 256);
        assert_eq!(tile.area(), 256 * 256);
    }
    
    #[test]
    fn test_tile_overlaps() {
        let tile1 = Tile::new(0, 0, 256, 256);
        let tile2 = Tile::new(128, 128, 256, 256);
        let tile3 = Tile::new(300, 300, 256, 256);
        
        assert!(tile1.overlaps(&tile2));
        assert!(tile2.overlaps(&tile1));
        assert!(!tile1.overlaps(&tile3));
        assert!(!tile3.overlaps(&tile1));
    }
    
    #[test]
    fn test_tile_grid_creation() {
        let grid = TileGrid::new(1000, 800, 256);
        
        assert_eq!(grid.tile_size(), 256);
        assert_eq!(grid.tile_count(), 4 * 4); // ceil(1000/256) * ceil(800/256) = 4 * 4 = 16
        
        // Check first tile
        let first = grid.tile(0).unwrap();
        assert_eq!(first.x, 0);
        assert_eq!(first.y, 0);
        assert_eq!(first.width, 256);
        assert_eq!(first.height, 256);
        
        // Check last tile (bottom-right)
        let last = grid.tile(grid.tile_count() - 1).unwrap();
        assert_eq!(last.x, 768); // 3 * 256 = 768
        assert_eq!(last.y, 768); // 3 * 256 = 768
        assert_eq!(last.width, 232); // 1000 - 768 = 232
        assert_eq!(last.height, 32); // 800 - 768 = 32
    }
    
    #[test]
    fn test_tiles_in_viewport() {
        let grid = TileGrid::new(1000, 800, 256);
        
        // Viewport covering top-left corner
        let tiles = grid.tiles_in_viewport(0.0, 0.0, 300.0, 300.0);
        assert_eq!(tiles.len(), 4); // Should intersect with 4 tiles
        
        // Viewport covering entire image
        let tiles = grid.tiles_in_viewport(0.0, 0.0, 1000.0, 800.0);
        assert_eq!(tiles.len(), grid.tile_count());
        
        // Viewport outside image
        let tiles = grid.tiles_in_viewport(2000.0, 2000.0, 100.0, 100.0);
        assert_eq!(tiles.len(), 0);
    }
}
