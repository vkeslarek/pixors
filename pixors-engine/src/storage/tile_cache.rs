//! In-memory LRU cache for tiles.

use crate::error::Error;
use crate::image::Tile;
use crate::pixel::Rgba;
use crate::storage::{ImageSource, TileStore};
use half::f16;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Key for identifying a tile in the cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TileKey {
    tab_id: Uuid,
    x: u32,
    y: u32,
}

impl TileKey {
    fn new(tab_id: Uuid, tile: &Tile) -> Self {
        Self {
            tab_id,
            x: tile.x,
            y: tile.y,
        }
    }
}

/// In-memory LRU cache for tiles that are actively needed by the viewport.
/// Capacity is in number of tiles (e.g., 256 tiles × 256×256×8 bytes ≈ 128 MB).
#[derive(Debug)]
pub struct TileCache {
    cache: RwLock<LruCache<TileKey, Arc<Vec<Rgba<f16>>>>>,
}

impl TileCache {
    /// Creates a new tile cache with the given capacity (number of tiles).
    pub fn new(capacity: usize) -> Self {
        let max_tiles = capacity.max(1);
        Self {
            cache: RwLock::new(LruCache::new(NonZeroUsize::new(max_tiles).unwrap())),
        }
    }

    /// Get a tile, promoting from TileStore → RAM if needed.
    pub async fn get_or_load(
        &self,
        tab_id: Uuid,
        tile: &Tile,
        store: &TileStore,
        source: &dyn ImageSource,
    ) -> Result<Arc<Vec<Rgba<f16>>>, Error> {
        let key = TileKey::new(tab_id, tile);
        
        // 1. Check RAM cache
        {
            let mut cache = self.cache.write().await;
            if let Some(cached) = cache.get(&key) {
                return Ok(Arc::clone(cached));
            }
        }
        
        // 2. Check disk store
        if let Some(disk_data) = store.get(tile).await? {
            let arc_data = Arc::new(disk_data);
            let mut cache = self.cache.write().await;
            cache.put(key, Arc::clone(&arc_data));
            return Ok(arc_data);
        }
        
        // 3. Decode from source
        let decoded = source.decode_tile(tile.x, tile.y, tile.width, tile.height).await?;
        // Store to disk
        store.put(tile, &decoded).await?;
        let arc_data = Arc::new(decoded);
        let mut cache = self.cache.write().await;
        cache.put(key, Arc::clone(&arc_data));
        Ok(arc_data)
    }

    /// Evict all tiles for a tab.
    pub async fn evict_tab(&self, tab_id: &Uuid) {
        let mut cache = self.cache.write().await;
        let keys_to_remove: Vec<_> = cache
            .iter()
            .filter(|(k, _)| k.tab_id == *tab_id)
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys_to_remove {
            cache.pop(&key);
        }
    }

    /// Invalidate a specific tile (remove from cache).
    pub async fn invalidate_tile(&self, tab_id: &Uuid, tile: &Tile) {
        let key = TileKey::new(*tab_id, tile);
        let mut cache = self.cache.write().await;
        cache.pop(&key);
    }

    /// Invalidate multiple tiles (remove from cache).
    pub async fn invalidate_tiles(&self, tab_id: &Uuid, tiles: &[Tile]) {
        let mut cache = self.cache.write().await;
        for tile in tiles {
            let key = TileKey::new(*tab_id, tile);
            cache.pop(&key);
        }
    }

    /// Clear the entire cache.
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Returns current number of cached tiles.
    pub async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Returns true if cache is empty.
    pub async fn is_empty(&self) -> bool {
        let cache = self.cache.read().await;
        cache.is_empty()
    }
}

impl Default for TileCache {
    fn default() -> Self {
        Self::new(256) // default capacity: 256 tiles
    }
}