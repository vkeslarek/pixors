//! MIP pyramid with lazy tile-store-backed generation.

use crate::error::Error;
use crate::image::{TileCoord, TileGrid};
use crate::pixel::Rgba;
use crate::storage::TileStore;
use half::f16;
use uuid::Uuid;

/// One level in a MIP pyramid with its own tile storage.
#[derive(Debug)]
pub struct MipLevel {
    /// Level index (0 = full resolution, 1 = half, etc.)
    pub index: usize,
    /// Image width at this level (pixels).
    pub width: u32,
    /// Image height at this level (pixels).
    pub height: u32,
    /// Scale factor relative to full resolution (0.5, 0.25, ...)
    pub scale: f32,
    /// Tile grid for this level.
    pub tile_grid: TileGrid,
    /// Tile store for this level (disk-backed).
    pub tile_store: TileStore,
    /// Whether this level has been generated (tiles populated).
    pub generated: bool,
}

impl MipLevel {
    /// Creates a new MIP level with empty tile store.
    pub fn new(
        index: usize,
        width: u32,
        height: u32,
        scale: f32,
        tile_size: u32,
        tab_id: &Uuid,
    ) -> Result<Self, Error> {
        let tile_grid = TileGrid::new(width, height, tile_size);
        // Each MIP level gets its own subdirectory to prevent tile file collisions.
        let tile_store = TileStore::new_with_subdir(
            tab_id,
            &format!("mip_{}", index),
            tile_size,
            width,
            height,
        )?;
        
        Ok(Self {
            index,
            width,
            height,
            scale,
            tile_grid,
            tile_store,
            generated: false,
        })
    }
    
    /// Returns tile at given tile coordinates (tx, ty).
    pub fn tile_at(&self, tx: u32, ty: u32) -> Option<&TileCoord> {
        self.tile_grid.tile_at(tx, ty)
    }

    /// Returns all tiles in this level.
    pub fn tiles(&self) -> impl Iterator<Item = &TileCoord> {
        self.tile_grid.tiles()
    }
}

/// Pre-computed downscaled versions of the image for fast zoom-out rendering.
/// Each level has its own tile store for lazy generation.
#[derive(Debug)]
pub struct MipPyramid {
    /// Level 0 = full resolution (not stored here, uses main tile store).
    /// Stored levels start at 1 (half resolution).
    levels: Vec<MipLevel>,
    /// Tile size used for all levels.
    #[allow(dead_code)]
    tile_size: u32,
    /// Tab ID for tile store paths.
    tab_id: Uuid,
}

impl MipPyramid {
    /// Creates a new MIP pyramid with empty tile stores.
    /// Level 0 is implicit (full resolution), not stored here.
    pub fn new(
        src_width: u32,
        src_height: u32,
        tile_size: u32,
        tab_id: &Uuid,
    ) -> Result<Self, Error> {
        let mut levels = Vec::new();
        let mut width = src_width.max(1);
        let mut height = src_height.max(1);
        let mut scale = 0.5_f32;
        let mut index = 1; // Level 1 = half resolution
        
        while width > 1 || height > 1 {
            width = (width / 2).max(1);
            height = (height / 2).max(1);
            
            let level = MipLevel::new(index, width, height, scale, tile_size, tab_id)?;
            levels.push(level);
            
            scale *= 0.5;
            index += 1;
        }
        
        Ok(Self {
            levels,
            tile_size,
            tab_id: *tab_id,
        })
    }
    
    /// Returns the number of MIP levels (excluding level 0).
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }
    
    /// Returns a reference to a specific MIP level (1-indexed).
    pub fn level(&self, index: usize) -> Option<&MipLevel> {
        self.levels.get(index.saturating_sub(1))
    }
    
    /// Returns a mutable reference to a specific MIP level (1-indexed).
    pub fn level_mut(&mut self, index: usize) -> Option<&mut MipLevel> {
        self.levels.get_mut(index.saturating_sub(1))
    }
    
    /// Returns all levels.
    pub fn levels(&self) -> &[MipLevel] {
        &self.levels
    }
    
    /// Ensures the appropriate MIP level for the given zoom is generated.
    /// Uses `generate_from_mip0` which runs box-filter downsampling via rayon.
    pub async fn ensure_level_for_zoom(
        &mut self,
        zoom: f32,
        mip0: &TileStore,
    ) -> Result<(), Error> {
        let level_index = mip_level_for_zoom(zoom);
        if level_index == 0 {
            return Ok(()); // Use base level
        }
        
        // Regenerate from MIP 0 using parallel downsampling
        // Only if the requested level doesn't exist yet
        if self.level(level_index).map(|l| l.generated).unwrap_or(false) {
            return Ok(());
        }
        
        let regenerated = generate_from_mip0(mip0, &self.tab_id)?;
        self.levels = regenerated.levels;
        Ok(())
    }
}

/// Select the appropriate MIP level for the current zoom.
/// Returns 0 for base level (full-res), 1 for half, etc.
pub fn mip_level_for_zoom(zoom: f32) -> usize {
    if zoom >= 0.5 {
        return 0;
    }
    (-(zoom.max(1e-6).log2())).floor() as usize
}

// ---------------------------------------------------------------------------
// Parallel MIP generation (rayon)
// ---------------------------------------------------------------------------

use rayon::prelude::*;

/// Generate all MIP levels from MIP 0 tile store.
/// Runs box-filter downsampling in parallel via rayon.
pub fn generate_from_mip0(
    mip0: &TileStore,
    tab_id: &Uuid,
) -> Result<MipPyramid, Error> {
    let tile_size = mip0.tile_size();
    let mut width = mip0.image_width();
    let mut height = mip0.image_height();

    let mut levels: Vec<MipLevel> = Vec::new();
    let mut level_idx = 1u32;

    loop {
        width = (width + 1).max(1) / 2;
        height = (height + 1).max(1) / 2;

        if (width <= tile_size || height <= tile_size) && level_idx > 1 {
            break;
        }

        let store = TileStore::new_with_subdir(
            tab_id,
            &format!("mip_{}", level_idx),
            tile_size,
            width,
            height,
        )?;

        downsample_level_rayon(&levels.last().map(|l: &MipLevel| &l.tile_store).unwrap_or(mip0), &store, level_idx)?;

        let scale = 0.5_f32.powi(level_idx as i32);
        let tile_grid = TileGrid::new(width, height, tile_size);
        levels.push(MipLevel {
            index: level_idx as usize,
            width,
            height,
            scale,
            tile_grid,
            tile_store: store,
            generated: true,
        });

        level_idx += 1;
    }

    Ok(MipPyramid {
        levels,
        tile_size,
        tab_id: *tab_id,
    })
}

/// Downsample one level: 2×2 box filter, parallel over destination tiles.
fn downsample_level_rayon(
    src: &TileStore,
    dst: &TileStore,
    dst_mip: u32,
) -> Result<(), Error> {
    let tiles_x = (dst.image_width() + dst.tile_size() - 1) / dst.tile_size();
    let tiles_y = (dst.image_height() + dst.tile_size() - 1) / dst.tile_size();
    let tile_size = dst.tile_size();
    let src_w = src.image_width();
    let src_h = src.image_height();

    (0..tiles_y).into_par_iter().try_for_each(|ty| {
        (0..tiles_x).try_for_each(|tx| {
            let coord = TileCoord::new(
                dst_mip, tx, ty,
                tile_size, dst.image_width(), dst.image_height(),
            );
            let mut data = Vec::with_capacity(coord.pixel_count());

            for dy in 0..coord.height {
                for dx in 0..coord.width {
                    let sx = coord.px + dx;
                    let sy = coord.py + dy;
                    // 2×2 box filter: clamp to source bounds (edge tiles)
                    let x0 = (sx * 2).min(src_w.saturating_sub(1));
                    let x1 = (sx * 2 + 1).min(src_w.saturating_sub(1));
                    let y0 = (sy * 2).min(src_h.saturating_sub(1));
                    let y1 = (sy * 2 + 1).min(src_h.saturating_sub(1));
                    let p00 = src.sample(x0, y0)?;
                    let p10 = src.sample(x1, y0)?;
                    let p01 = src.sample(x0, y1)?;
                    let p11 = src.sample(x1, y1)?;
                    data.push(avg4(p00, p10, p01, p11));
                }
            }

            dst.write_tile_blocking(&crate::image::Tile::new(coord, data))
        })
    })
}

#[inline]
fn avg4(a: Rgba<f16>, b: Rgba<f16>, c: Rgba<f16>, d: Rgba<f16>) -> Rgba<f16> {
    macro_rules! avg { ($ch:ident) => {
        f16::from_f32((a.$ch.to_f32() + b.$ch.to_f32() + c.$ch.to_f32() + d.$ch.to_f32()) * 0.25)
    }}
    Rgba { r: avg!(r), g: avg!(g), b: avg!(b), a: avg!(a) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyramid_levels_descend_to_1x1() {
        let tab_id = uuid::Uuid::new_v4();
        let p = MipPyramid::new(4148, 5531, 256, &tab_id).unwrap();
        let last = p.levels().last().unwrap();
        assert_eq!((last.width, last.height), (1, 1));
    }

    #[test]
    fn zoom_level_selection() {
        assert_eq!(mip_level_for_zoom(1.0), 0);
        assert_eq!(mip_level_for_zoom(0.5), 0);
        assert_eq!(mip_level_for_zoom(0.25), 2);
    }

    #[test]
    fn generate_from_mip0_test() {
        use crate::image::Tile;
        use crate::storage::TileStore;

        let tab_id = uuid::Uuid::new_v4();
        let tile_size = 16;

        // Create MIP 0: 32×32, grey ramp, 4 tiles of 16×16
        let mip0 = TileStore::new(&tab_id, tile_size, 32, 32).unwrap();
        for ty in 0..2 {
            for tx in 0..2 {
                let coord = TileCoord::new(0, tx, ty, tile_size, 32, 32);
                let mut data = Vec::with_capacity(coord.pixel_count());
                for y in 0..coord.height {
                    for x in 0..coord.width {
                        let v = f16::from_f32(((coord.py + y) * 32 + (coord.px + x)) as f32 / (32.0 * 32.0));
                        data.push(Rgba::new(v, v, v, f16::ONE));
                    }
                }
                mip0.write_tile_blocking(&Tile::new(coord, data)).unwrap();
            }
        }

        // Generate MIP levels
        let pyramid = generate_from_mip0(&mip0, &tab_id).unwrap();
        assert!(!pyramid.levels.is_empty(), "should have at least one MIP level");

        // Level 1: 16×16, 1 tile
        let l1 = &pyramid.levels[0];
        assert_eq!(l1.width, 16, "level 1 width");
        assert_eq!(l1.height, 16, "level 1 height");
        assert!(l1.generated);

        // Read first pixel of MIP 1 → should be average of 4 MIP 0 pixels at (0,0), (1,0), (0,1), (1,1)
        let l1_coord = TileCoord::new(1, 0, 0, tile_size, 16, 16);
        let l1_tile = l1.tile_store.read_tile(l1_coord).unwrap().unwrap();
        let p00 = mip0.sample(0, 0).unwrap();
        let p10 = mip0.sample(1, 0).unwrap();
        let p01 = mip0.sample(0, 1).unwrap();
        let p11 = mip0.sample(1, 1).unwrap();
        let expected = avg4(p00, p10, p01, p11);
        let actual = l1_tile.data[0];
        let diff = (actual.r.to_f32() - expected.r.to_f32()).abs();
        assert!(diff < 0.001, "MIP 1 first pixel should equal avg of 4 MIP 0 pixels: diff={diff}");
    }
}

