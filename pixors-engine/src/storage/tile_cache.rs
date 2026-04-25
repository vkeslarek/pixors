//! In-memory caches for tiles.
//!
//! Two separate LRU caches:
//! - `acescg`: ACEScg f16 tiles (source of truth for conversion & MIP gen)
//! - `display`: sRGB u8 tiles (ready for WebSocket send)
//!
//! Both use `(Uuid, TileCoord)` as key (tab_id, coord).

use crate::color::ColorConversion;
use crate::error::Error;
use crate::image::{Tile, TileCoord};
use crate::pixel::Rgba;
use crate::storage::TileStore;
use half::f16;
use lru::LruCache;
use parking_lot::RwLock;
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use uuid::Uuid;

/// Capacity limits (number of tiles per cache).
const ACESCG_CAPACITY: usize = 128;   // ~16 MB for 256×256 tiles
const DISPLAY_CAPACITY: usize = 256;  // ~64 MB for 256×256 tiles (u8 RGBA)

/// In-memory tile caches for ACEScg and display tiles.
///
/// LRU eviction: least-recently-used tiles are evicted when capacity exceeded.
pub struct TileCache {
    /// ACEScg f16 tiles: (tab_id, TileCoord) → Arc<Vec<Rgba<f16>>>
    acescg: RwLock<LruCache<(Uuid, TileCoord), Arc<Vec<Rgba<f16>>>>>,
    /// sRGB u8 display tiles: (tab_id, TileCoord) → Arc<Vec<u8>>
    display: RwLock<LruCache<(Uuid, TileCoord), Arc<Vec<u8>>>>,
}

impl fmt::Debug for TileCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileCache")
            .field("acescg_len", &self.acescg.read().len())
            .field("display_len", &self.display.read().len())
            .finish()
    }
}

impl TileCache {
    pub fn new() -> Self {
        Self {
            acescg: RwLock::new(
                LruCache::new(NonZeroUsize::new(ACESCG_CAPACITY).unwrap())
            ),
            display: RwLock::new(
                LruCache::new(NonZeroUsize::new(DISPLAY_CAPACITY).unwrap())
            ),
        }
    }

    /// Get a display tile (sRGB u8). Load + convert from TileStore on miss.
    pub async fn get_display(
        &self,
        tab_id: Uuid,
        coord: TileCoord,
        store: &TileStore,
        conv: &ColorConversion,
    ) -> Result<Arc<Vec<u8>>, Error> {
        let key = (tab_id, coord);

        // 1. Display cache hit
        {
            let mut cache = self.display.write();
            if let Some(v) = cache.get(&key) {
                return Ok(v.clone());
            }
        }

        // 2. ACEScg cache hit → convert to sRGB
        {
            let mut acescg_cache = self.acescg.write();
            if let Some(acescg_tile) = acescg_cache.get(&key) {
                let tile = Tile::new(coord, acescg_tile.as_ref().clone());
                let display = tile.to_srgb_u8(conv);
                let data = display.data.clone();
                self.display.write().put(key, data.clone());
                return Ok(data);
            }
        }

        // 3. Miss → load from TileStore, cache both
        let _sw = crate::debug_stopwatch!("get_display:miss");
        let disk_tile = store
            .read_tile(coord)?
            .ok_or_else(|| Error::invalid_param(format!("tile not in store: {:?}", coord)))?;

        let acescg_data = disk_tile.data.clone();
        self.acescg.write().put(key, acescg_data.clone());

        let display_data = disk_tile.to_srgb_u8(conv).data;
        self.display.write().put(key, display_data.clone());

        Ok(display_data)
    }

    /// Get ACEScg tile (f16). Load from TileStore on miss.
    pub async fn get_acescg(
        &self,
        tab_id: Uuid,
        coord: TileCoord,
        store: &TileStore,
    ) -> Result<Arc<Vec<Rgba<f16>>>, Error> {
        let key = (tab_id, coord);

        {
            let mut cache = self.acescg.write();
            if let Some(v) = cache.get(&key) {
                return Ok(v.clone());
            }
        }

        let disk_tile = store
            .read_tile(coord)?
            .ok_or_else(|| Error::invalid_param(format!("tile not in store: {:?}", coord)))?;

        let data = disk_tile.data.clone();
        self.acescg.write().put(key, data.clone());
        Ok(data)
    }

    /// Invalidate a display tile (keep ACEScg for MIP regeneration).
    pub fn invalidate_display(&self, tab_id: Uuid, coord: TileCoord) {
        self.display.write().pop(&(tab_id, coord));
    }

    /// Invalidate all tiles for a MIP level (display + ACEScg).
    pub fn invalidate_mip(&self, tab_id: Uuid, mip_level: u32) {
        let keys_to_remove: Vec<_> = self.display.read().iter()
            .filter(|(k, _)| k.0 == tab_id && k.1.mip_level == mip_level)
            .map(|(k, _)| *k)
            .collect();
        for key in keys_to_remove {
            self.display.write().pop(&key);
        }

        let keys_to_remove: Vec<_> = self.acescg.read().iter()
            .filter(|(k, _)| k.0 == tab_id && k.1.mip_level == mip_level)
            .map(|(k, _)| *k)
            .collect();
        for key in keys_to_remove {
            self.acescg.write().pop(&key);
        }
    }

    /// Invalidate a specific tile in both caches.
    pub fn invalidate_tile(&self, tab_id: Uuid, coord: TileCoord) {
        self.display.write().pop(&(tab_id, coord));
        self.acescg.write().pop(&(tab_id, coord));
    }

    /// Evict all tiles for a tab.
    pub fn evict_tab(&self, tab_id: &Uuid) {
        let keys_to_remove: Vec<_> = self.display.read().iter()
            .filter(|(k, _)| k.0 == *tab_id)
            .map(|(k, _)| *k)
            .collect();
        for key in keys_to_remove {
            self.display.write().pop(&key);
        }

        let keys_to_remove: Vec<_> = self.acescg.read().iter()
            .filter(|(k, _)| k.0 == *tab_id)
            .map(|(k, _)| *k)
            .collect();
        for key in keys_to_remove {
            self.acescg.write().pop(&key);
        }
    }

    /// Clear all caches.
    pub fn clear(&self) {
        self.display.write().clear();
        self.acescg.write().clear();
    }
}

impl Default for TileCache {
    fn default() -> Self {
        Self::new()
    }
}
