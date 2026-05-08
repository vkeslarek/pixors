use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use pixors_executor::data::tile::TileGridPos;

#[derive(Debug)]
pub struct CachedTile {
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Arc<Vec<u8>>,
    pub generation: u64,
}

/// Two-tier tile cache.
///
/// `base` holds gen=0 tiles written by OpenFile / MipFetch — these are the
/// source-of-truth pixels and are **never** evicted by preview pipelines.
///
/// `overlay` holds gen>0 tiles written by preview pipelines (e.g. blur
/// preview). The viewport renders the overlay tile if one exists for a
/// position, falling back to base. `clear_generation` removes overlay tiles;
/// it never touches base.
///
/// This invariant ensures the blur-preview source always finds its gen=0
/// input tiles regardless of how many preview cycles have run.
#[derive(Debug)]
pub struct ViewportCache {
    base: HashMap<TileGridPos, CachedTile>,
    overlay: HashMap<TileGridPos, CachedTile>,
    pending: HashSet<TileGridPos>,
    new_img: Option<(u32, u32)>,
    pub active_dims: (u32, u32),
    pub active_mip: u32,
}

impl ViewportCache {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            base: HashMap::new(),
            overlay: HashMap::new(),
            pending: HashSet::new(),
            new_img: None,
            active_dims: (1, 1),
            active_mip: 0,
        }))
    }

    /// Insert a tile.
    ///
    /// gen=0 → written to `base` (always; old base tiles are freely replaced
    /// by fresher fetches of the same position).
    ///
    /// gen>0 → written to `overlay`. Silently dropped if an overlay tile with
    /// a HIGHER generation already exists at that position (prevents a stale
    /// pipeline from rolling back a newer preview).
    pub fn insert(&mut self, generation: u64, key: TileGridPos, tile: CachedTile) {
        if generation == 0 {
            self.base.insert(key, tile);
            self.pending.insert(key);
        } else {
            if let Some(existing) = self.overlay.get(&key)
                && existing.generation > generation
            {
                return;
            }
            self.overlay.insert(key, tile);
            self.pending.insert(key);
        }
    }

    /// Drains pending keys for a MIP level (marks them as uploaded to GPU texture).
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

    /// Lookup a tile for display: overlay wins over base.
    pub fn get(&self, key: &TileGridPos) -> Option<&CachedTile> {
        self.overlay.get(key).or_else(|| self.base.get(key))
    }

    pub fn tiles_in_range(
        &self,
        mip: u32,
        range: &pixors_executor::source::cache_reader::TileRange,
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
        range: &pixors_executor::source::cache_reader::TileRange,
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

    /// Clear everything — call before loading a new image.
    pub fn clear_all(&mut self) {
        self.base.clear();
        self.overlay.clear();
        self.pending.clear();
        self.new_img = None;
        self.active_dims = (1, 1);
        self.active_mip = 0;
    }

    /// Remove all overlay tiles for a preview generation. Never touches base.
    pub fn clear_generation(&mut self, generation: u64) {
        if generation > 0 {
            self.overlay.retain(|_, t| t.generation != generation);
        }
    }

    /// Return all base (gen=0) tiles at a mip level, used by the blur-preview
    /// source to read the unmodified image pixels.
    pub fn tiles_at_mip(&self, mip: u32, generation: u64) -> Vec<(TileGridPos, &CachedTile)> {
        let map = if generation == 0 {
            &self.base
        } else {
            &self.overlay
        };
        map.iter()
            .filter(|(k, v)| k.mip_level == mip && v.generation == generation)
            .map(|(k, v)| (*k, v))
            .collect()
    }

    pub fn signal_new_img(&mut self, w: u32, h: u32) {
        self.new_img = Some((w, h));
        self.active_dims = (w, h);
    }

    pub fn take_new_img(&mut self) -> Option<(u32, u32)> {
        self.new_img.take()
    }
}
