use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use pixors_engine::data::tile::TileGridPos;

#[derive(Debug)]
pub struct CachedTile {
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Arc<Vec<u8>>,
    pub layer: u64,
}

/// Two-tier tile cache.
///
/// `base` holds gen=0 tiles (source-of-truth, never overwritten by previews).
/// `overlay` holds gen>0 tiles from preview pipelines (e.g. blur preview).
/// Viewport renders overlay over base. `clear_generation` only touches overlay.
#[derive(Debug)]
pub struct TileCache {
    base: HashMap<TileGridPos, CachedTile>,
    overlay: HashMap<TileGridPos, CachedTile>,
    pending: HashSet<TileGridPos>,
    new_img: Option<(u32, u32)>,
    pub active_dims: (u32, u32),
    pub active_mip: u32,
    pub active_generation: u64,
}

impl TileCache {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            base: HashMap::new(),
            overlay: HashMap::new(),
            pending: HashSet::new(),
            new_img: None,
            active_dims: (1, 1),
            active_mip: 0,
            active_generation: 0,
        }))
    }

    pub fn insert(&mut self, generation: u64, version: u64, key: TileGridPos, tile: CachedTile) {
        if generation == 0 {
            // Base writes carry a doc-version stamp. Reject tiles from older
            // pipelines so a stale recomposite (e.g. behind an opacity drag)
            // can't overwrite tiles produced by the current document state.
            if let Some(existing) = self.base.get(&key)
                && existing.layer > version
            {
                return;
            }
            let is_new = !self.base.contains_key(&key);
            let mut tile = tile;
            tile.layer = version;
            self.base.insert(key, tile);
            self.pending.insert(key);
            if is_new {
                let mip_count = self
                    .base
                    .keys()
                    .filter(|k| k.mip_level == key.mip_level)
                    .count();
                tracing::debug!(
                    "[tile_cache] insert gen=0 mip={} tx={} ty={} → {} total at this mip",
                    key.mip_level,
                    key.tx,
                    key.ty,
                    mip_count,
                );
            }
        } else {
            if let Some(existing) = self.overlay.get(&key)
                && existing.layer > generation
            {
                return;
            }
            self.overlay.insert(key, tile);
            self.pending.insert(key);
        }
    }

    pub fn take_pending_keys_for_mip(&mut self, mip: u32) -> Vec<TileGridPos> {
        let keys: Vec<TileGridPos> = self
            .pending
            .iter()
            .filter(|k| k.mip_level == mip)
            .copied()
            .collect();
        self.pending.retain(|k| k.mip_level != mip);
        keys
    }

    pub fn get(&self, key: &TileGridPos) -> Option<&CachedTile> {
        if self.active_generation > 0 {
            if let Some(t) = self.overlay.get(key)
                && t.layer == self.active_generation
            {
                return Some(t);
            }
        }
        self.base.get(key)
    }

    pub fn tiles_in_range(
        &self,
        mip: u32,
        range: &pixors_ops::source::cache_reader::TileRange,
    ) -> Vec<(TileGridPos, &CachedTile)> {
        let mut res = Vec::new();
        for ty in range.ty_start..range.ty_end {
            for tx in range.tx_start..range.tx_end {
                let pos = TileGridPos {
                    mip_level: mip,
                    tx,
                    ty,
                };
                if let Some(tile) = self.get(&pos) {
                    res.push((pos, tile));
                }
            }
        }
        res
    }

    pub fn has_mip(&self, mip: u32) -> bool {
        self.base.keys().any(|k| k.mip_level == mip)
            || self.overlay.keys().any(|k| k.mip_level == mip)
    }

    pub fn has_all_tiles(
        &self,
        mip: u32,
        range: &pixors_ops::source::cache_reader::TileRange,
    ) -> bool {
        for ty in range.ty_start..range.ty_end {
            for tx in range.tx_start..range.tx_end {
                let pos = TileGridPos {
                    mip_level: mip,
                    tx,
                    ty,
                };
                if !self.base.contains_key(&pos) && !self.overlay.contains_key(&pos) {
                    return false;
                }
            }
        }
        true
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn set_active_mip(&mut self, mip: u32) {
        self.active_mip = mip;
    }

    pub fn clear_all(&mut self) {
        self.base.clear();
        self.overlay.clear();
        self.pending.clear();
        self.new_img = None;
        self.active_dims = (1, 1);
        self.active_mip = 0;
        self.active_generation = 0;
    }

    pub fn clear_generation(&mut self, generation: u64) {
        if generation > 0 {
            self.overlay.clear();
        }
    }

    pub fn tiles_at_mip(&self, mip: u32, generation: u64) -> Vec<(TileGridPos, &CachedTile)> {
        if generation == 0 {
            return self
                .base
                .iter()
                .filter(|(k, v)| k.mip_level == mip && v.layer == generation)
                .map(|(k, v)| (*k, v))
                .collect();
        }
        // Overlay generation: return overlay tiles plus base tiles for positions
        // not yet written in the overlay (prevents black gaps during partial preview).
        let mut result: Vec<(TileGridPos, &CachedTile)> = self
            .overlay
            .iter()
            .filter(|(k, _)| k.mip_level == mip)
            .map(|(k, v)| (*k, v))
            .collect();
        for (k, v) in &self.base {
            if k.mip_level == mip && !self.overlay.contains_key(k) {
                result.push((*k, v));
            }
        }
        result
    }

    pub fn signal_new_img(&mut self, w: u32, h: u32) {
        self.new_img = Some((w, h));
        self.active_dims = (w, h);
    }

    pub fn take_new_img(&mut self) -> Option<(u32, u32)> {
        self.new_img.take()
    }
}
