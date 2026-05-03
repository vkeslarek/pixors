use crate::image::{Tile, TileCoord};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::hash::Hash;

/// Identifies a tile within a MIP level for neighborhood accumulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NeighborhoodCoord {
    pub mip: u32,
    pub tx: u32,
    pub ty: u32,
}

impl NeighborhoodCoord {
    pub fn new(mip: u32, tx: u32, ty: u32) -> Self { Self { mip, tx, ty } }
    pub fn from_tile(tile: &TileCoord) -> Self { Self { mip: tile.mip_level, tx: tile.tx, ty: tile.ty } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeCondition {
    Clamp,
    Mirror,
    Transparent,
}

/// A center tile and its surrounding neighbors within a given radius.
/// Used by operations that need pixel data beyond tile boundaries (blur, etc.).
#[derive(Clone)]
pub struct Neighborhood<P: Clone + Send + Sync + 'static> {
    pub radius: u32,
    pub center: TileCoord,
    pub edge: EdgeCondition,
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
    tiles: HashMap<(i32, i32), Option<Arc<Tile<P>>>>,
}

impl<P: Clone + Send + Sync + 'static> Neighborhood<P> {
    pub fn new(
        center: TileCoord,
        radius: u32,
        image_width: u32,
        image_height: u32,
        tile_size: u32,
        edge: EdgeCondition,
    ) -> Self {
        Self {
            radius,
            center,
            edge,
            tiles: HashMap::new(),
            image_width,
            image_height,
            tile_size,
        }
    }

    /// The max number of tiles in this neighborhood (full grid).
    pub fn capacity(&self) -> usize {
        let r = self.radius as i32;
        ((2 * r + 1) * (2 * r + 1)) as usize
    }

    /// Whether all neighbor positions have been filled.
    pub fn is_complete(&self) -> bool {
        let r = self.radius as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                if !self.tiles.contains_key(&(dx, dy)) {
                    return false;
                }
            }
        }
        true
    }

    /// Insert a tile at the given tile-grid offset relative to the center.
    /// The offset is (dtx, dty) where (0,0) is the center tile.
    pub fn insert(&mut self, offset: (i32, i32), tile: Option<Arc<Tile<P>>>) {
        self.tiles.insert(offset, tile);
    }

    /// Total tiles contained (including center and neighbors).
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Lookup a tile by its grid offset relative to the center.
    /// Outer Option: whether the slot was filled. Inner Option: whether it
    /// holds an actual tile (None = explicitly empty / out-of-bounds).
    pub fn tile_at_offset(&self, dtx: i32, dty: i32) -> Option<&Option<Arc<Tile<P>>>> {
        self.tiles.get(&(dtx, dty))
    }

    /// Read a pixel at absolute image coordinates (px, py).
    /// Handles crossing tile boundaries and edge conditions.
    pub fn pixel_at(&self, px: u32, py: u32) -> Option<&P> {
        let tx = px / self.tile_size;
        let ty = py / self.tile_size;
        let lx = px % self.tile_size;
        let ly = py % self.tile_size;

        let dtx = tx as i32 - self.center.tx as i32;
        let dty = ty as i32 - self.center.ty as i32;

        if let Some(tile) = self.tiles.get(&(dtx, dty)) {
            match tile {
                Some(t) => {
                    let w = t.coord.width;
                    if lx < w {
                        t.data.get((ly * w + lx) as usize)
                    } else {
                        None
                    }
                }
                None => self.edge_pixel(px, py),
            }
        } else {
            self.edge_pixel(px, py)
        }
    }

    /// Compute grid positions that need to be loaded.
    /// Returns the absolute (tx, ty) tile coordinates that are missing.
    pub fn missing_tiles(&mut self) -> Vec<(u32, u32)> {
        let r = self.radius as i32;
        let mut missing = Vec::new();
        let tx = self.center.tx;
        let ty = self.center.ty;
        for dy in -r..=r {
            for dx in -r..=r {
                if !self.tiles.contains_key(&(dx, dy)) {
                    let gx = (tx as i32 + dx).max(0) as u32;
                    let gy = (ty as i32 + dy).max(0) as u32;
                    if gx * self.tile_size < self.image_width
                        && gy * self.tile_size < self.image_height
                    {
                        missing.push((gx, gy));
                    } else {
                        self.tiles.insert((dx, dy), None);
                    }
                }
            }
        }
        missing
    }

    /// Number of grid positions in each dimension.
    pub fn grid_size(&self) -> u32 {
        2 * self.radius + 1
    }

    fn edge_pixel(&self, px: u32, py: u32) -> Option<&P> {
        match self.edge {
            EdgeCondition::Clamp => {
                let cx = px.clamp(0, self.image_width.saturating_sub(1));
                let cy = py.clamp(0, self.image_height.saturating_sub(1));
                if cx == px && cy == py {
                    None
                } else {
                    self.pixel_at(cx, cy)
                }
            }
            EdgeCondition::Mirror => {
                let mx = if px >= self.image_width {
                    2 * self.image_width - px - 1
                } else {
                    px
                };
                let my = if py >= self.image_height {
                    2 * self.image_height - py - 1
                } else {
                    py
                };
                self.pixel_at(mx, my)
            }
            EdgeCondition::Transparent => None,
        }
    }
}
