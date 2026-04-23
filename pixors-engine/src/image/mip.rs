//! MIP pyramid with lazy tile-store-backed generation.

use crate::error::Error;
use crate::image::{Tile, TileGrid};
use crate::pixel::Rgba;
use crate::storage::{TileStore, TileCache, ImageSource};
use half::f16;
use std::sync::Arc;
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
    
    /// Returns tile at given coordinates (in level-relative pixels).
    pub fn tile_at(&self, x: u32, y: u32) -> Option<&Tile> {
        self.tile_grid.tiles().find(|t| t.x == x && t.y == y)
    }
    
    /// Returns all tiles in this level.
    pub fn tiles(&self) -> impl Iterator<Item = &Tile> {
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
    
    /// Generates a MIP level from its parent level using 2×2 box filtering.
    /// If the parent level hasn't been generated, recursively generates it.
    /// `parent_tile_store` is the tile store for level 0 (full resolution).
    pub async fn generate_level(
        &mut self,
        target_level_index: usize,
        tile_cache: &TileCache,
        parent_tile_store: &TileStore,
        parent_tile_grid: &TileGrid,
        source: &dyn ImageSource,
    ) -> Result<(), Error> {
        if target_level_index == 0 {
            return Ok(()); // Level 0 is the source, not generated
        }
        
        let target_level_index = target_level_index.min(self.levels.len());
        
        // Iteratively generate levels up to the target
        for curr_idx in 1..=target_level_index {
            if self.level(curr_idx).map(|l| l.generated).unwrap_or(true) {
                continue;
            }
            
            tracing::debug!("Generating MIP level {}", curr_idx);
            
            // Clone the tiles we need to generate so we don't hold a borrow on `self`
            let level = self.level(curr_idx).unwrap();
            let tiles: Vec<Tile> = level.tiles().copied().collect();
            let tile_size = self.tile_size;
            
            for tile in tiles {
                let parent_coords = [
                    (tile.x * 2, tile.y * 2),
                    (tile.x * 2 + tile_size, tile.y * 2),
                    (tile.x * 2, tile.y * 2 + tile_size),
                    (tile.x * 2 + tile_size, tile.y * 2 + tile_size),
                ];
                
                // Fetch the 4 parent tiles
                let mut p_data: [Option<(Arc<Vec<Rgba<f16>>>, u32, u32)>; 4] = [None, None, None, None];
                
                for (i, &(px, py)) in parent_coords.iter().enumerate() {
                    let fetched = if curr_idx == 1 {
                        if let Some(t) = parent_tile_grid.tiles().find(|t| t.x == px && t.y == py) {
                            Some((
                                tile_cache.get_or_load(self.tab_id, t, parent_tile_store, source).await?,
                                t.width,
                                t.height
                            ))
                        } else {
                            None
                        }
                    } else {
                        let p_level = self.level(curr_idx - 1).unwrap();
                        if let Some(t) = p_level.tile_at(px, py) {
                            if let Some(d) = p_level.tile_store.get(t).await? {
                                Some((Arc::new(d), t.width, t.height))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    p_data[i] = fetched;
                }
                
                // 2x2 Box Filter
                let mut new_data = Vec::with_capacity((tile.width * tile.height) as usize);
                for y in 0..tile.height {
                    for x in 0..tile.width {
                        let mut r = 0.0;
                        let mut g = 0.0;
                        let mut b = 0.0;
                        let mut a = 0.0;
                        let mut count = 0;

                        for dy in 0..2 {
                            for dx in 0..2 {
                                let abs_px = x * 2 + dx;
                                let abs_py = y * 2 + dy;
                                
                                let q_x = if abs_px >= tile_size { 1 } else { 0 };
                                let q_y = if abs_py >= tile_size { 1 } else { 0 };
                                let q_idx = q_y * 2 + q_x;
                                
                                if let Some((p_pixels, pw, ph)) = &p_data[q_idx as usize] {
                                    let local_px = abs_px % tile_size;
                                    let local_py = abs_py % tile_size;
                                    
                                    if local_px < *pw && local_py < *ph {
                                        let idx = (local_py * *pw + local_px) as usize;
                                        let pixel = p_pixels[idx];
                                        r += pixel.r.to_f32();
                                        g += pixel.g.to_f32();
                                        b += pixel.b.to_f32();
                                        a += pixel.a.to_f32();
                                        count += 1;
                                    }
                                }
                            }
                        }
                        
                        let inv_count = if count > 0 { 1.0 / count as f32 } else { 0.0 };
                        new_data.push(Rgba::new(
                            f16::from_f32(r * inv_count),
                            f16::from_f32(g * inv_count),
                            f16::from_f32(b * inv_count),
                            f16::from_f32(a * inv_count),
                        ));
                    }
                }
                
                // Store generated tile
                self.level_mut(curr_idx).unwrap().tile_store.put(&tile, &new_data).await?;
            }
            
            self.level_mut(curr_idx).unwrap().generated = true;
            tracing::debug!("MIP level {} generated successfully", curr_idx);
        }
        
        Ok(())
    }
    
    /// Ensures the appropriate MIP level for the given zoom is generated.
    pub async fn ensure_level_for_zoom(
        &mut self,
        zoom: f32,
        tile_cache: &TileCache,
        parent_tile_store: &TileStore,
        parent_tile_grid: &TileGrid,
        source: &dyn ImageSource,
    ) -> Result<Option<&MipLevel>, Error> {
        let level_index = mip_level_for_zoom(zoom);
        if level_index == 0 {
            return Ok(None); // Use base level
        }
        
        self.generate_level(level_index, tile_cache, parent_tile_store, parent_tile_grid, source).await?;
        Ok(self.level(level_index))
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
}

