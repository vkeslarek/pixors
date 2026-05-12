use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// Tile key: (mip_level, tx, ty)
type TileKey = (u32, u32, u32);

struct LruEntry {
    data: Vec<u8>,
    #[allow(dead_code)]
    last_access: Instant,
    size: usize,
}

/// Per-layer disk-backed tile cache with an in-memory LRU.
///
/// Writes go to disk first, then LRU. Reads check LRU first, then fall
/// back to disk. On drop, the entire cache directory is removed.
pub struct DiskCache {
    cache_dir: PathBuf,
    lru: Mutex<LruState>,
    max_memory: usize,
}

impl fmt::Debug for DiskCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiskCache")
            .field("cache_dir", &self.cache_dir)
            .field("max_memory", &self.max_memory)
            .finish()
    }
}

struct LruState {
    entries: HashMap<TileKey, LruEntry>,
    mem_used: usize,
}

impl DiskCache {
    /// `max_memory` is the approximate byte limit for the in-memory LRU.
    /// Exceeding this triggers eviction of least-recently-used entries
    /// (eviction happens on write, not on a background thread).
    pub fn new(cache_dir: PathBuf, max_memory: usize) -> Self {
        Self {
            cache_dir,
            lru: Mutex::new(LruState {
                entries: HashMap::new(),
                mem_used: 0,
            }),
            max_memory,
        }
    }

    /// Returns the root cache directory (for constructing sub-paths like
    /// `mip_X/tile_X_Y.raw` that readers and writers use).
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Read a tile from the LRU (fast) or from disk (slow).
    /// Returns `None` if the tile doesn't exist.
    pub fn read_tile(&self, mip: u32, tx: u32, ty: u32) -> Option<Vec<u8>> {
        let key = (mip, tx, ty);

        // LRU hit
        {
            let mut state = self.lru.lock().unwrap();
            if let Some(entry) = state.entries.get_mut(&key) {
                entry.last_access = Instant::now();
                return Some(entry.data.clone());
            }
        }

        // Disk fallback
        let path = self.tile_path(mip, tx, ty);
        let data = fs::read(&path).ok()?;

        // Insert into LRU
        let size = data.len();
        let mut state = self.lru.lock().unwrap();
        state.evict_if_needed(size, self.max_memory);
        state.entries.insert(
            key,
            LruEntry {
                data: data.clone(),
                last_access: Instant::now(),
                size,
            },
        );
        state.mem_used += size;

        Some(data)
    }

    /// Write tile bytes to disk AND insert into the LRU.
    pub fn write_tile(&self, mip: u32, tx: u32, ty: u32, data: &[u8]) -> io::Result<()> {
        // Disk
        let path = self.tile_path(mip, tx, ty);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, data)?;

        // LRU
        let size = data.len();
        let key = (mip, tx, ty);
        let mut state = self.lru.lock().unwrap();
        state.evict_if_needed(size, self.max_memory);
        state.entries.insert(
            key,
            LruEntry {
                data: data.to_vec(),
                last_access: Instant::now(),
                size,
            },
        );
        state.mem_used += size;

        Ok(())
    }

    fn tile_path(&self, mip: u32, tx: u32, ty: u32) -> PathBuf {
        self.cache_dir
            .join(format!("mip_{}", mip))
            .join(format!("tile_{}_{}_{}.raw", mip, tx, ty))
    }

    /// Remove the entire cache directory from disk.
    pub fn cleanup(&self) {
        if let Err(e) = fs::remove_dir_all(&self.cache_dir) {
            tracing::warn!(
                "[pixors] DiskCache: failed to remove cache dir {}: {e}",
                self.cache_dir.display()
            );
        }
    }
}

impl LruState {
    fn evict_if_needed(&mut self, incoming: usize, max: usize) {
        while self.mem_used + incoming > max && !self.entries.is_empty() {
            // Evict the least-recently-used entry
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_access)
                .map(|(k, _)| *k);
            if let Some(key) = oldest
                && let Some(entry) = self.entries.remove(&key)
            {
                self.mem_used = self.mem_used.saturating_sub(entry.size);
            }
        }
    }
}
