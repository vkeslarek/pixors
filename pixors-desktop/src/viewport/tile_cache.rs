use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use pixors_executor::data::TileGridPos;

pub struct CachedTile {
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

pub struct ViewportCache {
    entries: HashMap<TileGridPos, CachedTile>,
    pending: HashSet<TileGridPos>,
    new_img: Option<(u32, u32)>,
}

impl ViewportCache {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            entries: HashMap::new(),
            pending: HashSet::new(),
            new_img: None,
        }))
    }

    pub fn insert(&mut self, key: TileGridPos, tile: CachedTile) {
        self.entries.insert(key, tile);
        self.pending.insert(key);
    }

    /// Drains pending keys for a MIP level (marks them as uploaded).
    pub fn take_pending_keys_for_mip(&mut self, mip: u32) -> Vec<TileGridPos> {
        let keys: Vec<TileGridPos> = self.pending.iter()
            .filter(|k| k.mip_level == mip)
            .copied()
            .collect();
        for k in &keys {
            self.pending.remove(k);
        }
        keys
    }

    pub fn get(&self, key: &TileGridPos) -> Option<&CachedTile> {
        self.entries.get(key)
    }

    /// All stored tiles for a MIP level — used on full re-upload (MIP switch or resize).
    pub fn all_for_mip(&self, mip: u32) -> Vec<(TileGridPos, &CachedTile)> {
        self.entries.iter()
            .filter(|(k, _)| k.mip_level == mip)
            .map(|(k, v)| (*k, v))
            .collect()
    }

    pub fn has_mip(&self, mip: u32) -> bool {
        self.entries.keys().any(|k| k.mip_level == mip)
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Clear everything — call before loading a new image.
    pub fn clear_all(&mut self) {
        self.entries.clear();
        self.pending.clear();
        self.new_img = None;
    }

    pub fn signal_new_img(&mut self, w: u32, h: u32) {
        self.new_img = Some((w, h));
    }

    pub fn take_new_img(&mut self) -> Option<(u32, u32)> {
        self.new_img.take()
    }
}
