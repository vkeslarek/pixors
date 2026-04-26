//! Disk-backed tile storage with hot LRU cache.

use crate::error::Error;
use crate::image::{Tile, TileCoord};
use crate::pixel::Rgba;
use half::f16;
use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

/// Persists tiles as explicit LE f16 blobs in a temp directory.
/// Each tile file: `{base_dir}/tile_{mip}_{tx}_{ty}.raw`
/// Internal hot cache keeps recently accessed tiles in RAM.
#[derive(Debug)]
pub struct TileStore {
    base_dir: PathBuf,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    mip_level: u32,
    hot_cache: RwLock<LruCache<TileCoord, Arc<Vec<Rgba<f16>>>>>,
    /// When false, Drop does NOT delete files. Used for read-only views sharing a path.
    auto_destroy: bool,
}

impl TileStore {
    /// Creates a new tile store at the given base directory.
    /// Creates the directory if it doesn't exist.
    pub fn new(
        base_dir: PathBuf,
        tile_size: u32,
        image_width: u32,
        image_height: u32,
    ) -> Result<Self, Error> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self {
            base_dir,
            tile_size,
            image_width,
            image_height,
            mip_level: 0,
            hot_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            auto_destroy: true,
        })
    }

    /// Open an existing TileStore path without taking ownership (no auto-cleanup on drop).
    pub fn open(
        base_dir: PathBuf,
        tile_size: u32,
        image_width: u32,
        image_height: u32,
    ) -> Result<Self, Error> {
        Ok(Self {
            base_dir,
            tile_size,
            image_width,
            image_height,
            mip_level: 0,
            hot_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            auto_destroy: false,
        })
    }

    /// Creates a tile store in a subdirectory of the given base directory.
    pub fn new_with_subdir(
        base_dir: PathBuf,
        subdir: &str,
        tile_size: u32,
        image_width: u32,
        image_height: u32,
    ) -> Result<Self, Error> {
        let base_dir = base_dir.join(subdir);
        std::fs::create_dir_all(&base_dir)?;

        let mip_level = if let Some(rest) = subdir.strip_prefix("mip_") {
            rest.parse::<u32>().unwrap_or(0)
        } else {
            0
        };

        Ok(Self {
            base_dir,
            tile_size,
            image_width,
            image_height,
            mip_level,
            hot_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            auto_destroy: true,
        })
    }

    pub fn base_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }
    pub fn image_width(&self) -> u32 {
        self.image_width
    }
    pub fn image_height(&self) -> u32 {
        self.image_height
    }
    pub fn mip_level(&self) -> u32 {
        self.mip_level
    }

    fn tile_path(&self, tile: &TileCoord) -> PathBuf {
        self.base_dir
            .join(format!("tile_{}_{}_{}.raw", tile.mip_level, tile.tx, tile.ty))
    }

    // ------------------------------------------------------------------
    // Internal LE f16 serialization
    // ------------------------------------------------------------------

    fn serialize_le(data: &[Rgba<f16>]) -> Vec<u8> {
        #[cfg(target_endian = "little")]
        {
            bytemuck::cast_slice::<Rgba<f16>, u8>(data).to_vec()
        }
        #[cfg(not(target_endian = "little"))]
        {
            let mut out = Vec::with_capacity(data.len() * 8);
            for px in data {
                out.extend_from_slice(&px.r.to_le_bytes());
                out.extend_from_slice(&px.g.to_le_bytes());
                out.extend_from_slice(&px.b.to_le_bytes());
                out.extend_from_slice(&px.a.to_le_bytes());
            }
            out
        }
    }

    fn deserialize_le(bytes: &[u8]) -> Vec<Rgba<f16>> {
        #[cfg(target_endian = "little")]
        {
            bytemuck::cast_slice::<u8, Rgba<f16>>(bytes).to_vec()
        }
        #[cfg(not(target_endian = "little"))]
        {
            bytes
                .chunks_exact(8)
                .map(|c| Rgba {
                    r: f16::from_le_bytes([c[0], c[1]]),
                    g: f16::from_le_bytes([c[2], c[3]]),
                    b: f16::from_le_bytes([c[4], c[5]]),
                    a: f16::from_le_bytes([c[6], c[7]]),
                })
                .collect()
        }
    }

    // ------------------------------------------------------------------
    // High-level API
    // ------------------------------------------------------------------

    /// Read tile from disk or hot cache. Returns None if not stored.
    pub fn read_tile(&self, coord: TileCoord) -> Result<Option<Tile<Rgba<f16>>>, Error> {
        // 1. Check hot cache
        {
            let mut cache = self.hot_cache.write();
            if let Some(cached) = cache.get(&coord) {
                return Ok(Some(Tile {
                    coord,
                    data: Arc::clone(cached),
                }));
            }
        }

        // 2. Read from disk
        let path = self.tile_path(&coord);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(Error::Io)?;
        let expected = coord.pixel_count() * 8;
        if bytes.len() != expected {
            return Err(Error::invalid_param("Tile file size mismatch"));
        }
        let pixels = Self::deserialize_le(&bytes);

        // 3. Populate hot cache
        let arc = Arc::new(pixels);
        {
            let mut cache = self.hot_cache.write();
            cache.put(coord, Arc::clone(&arc));
        }
        Ok(Some(Tile {
            coord,
            data: Arc::clone(&arc),
        }))
    }

    /// Write tile to disk, bypassing hot cache (disk is source of truth).
    pub fn write_tile_blocking(&self, tile: &Tile<Rgba<f16>>) -> Result<(), Error> {
        let path = self.tile_path(&tile.coord);
        let data_bytes = Self::serialize_le(&tile.data);
        std::fs::write(&path, data_bytes).map_err(Error::Io)?;
        // Keep tile in hot cache — next read hits memory not disk
        {
            let mut cache = self.hot_cache.write();
            cache.put(tile.coord, Arc::clone(&tile.data));
        }
        Ok(())
    }

    /// Read a single pixel from any tile. Returns pixel at image-space (x, y).
    pub fn sample(&self, x: u32, y: u32) -> Result<Rgba<f16>, Error> {
        if x >= self.image_width || y >= self.image_height {
            return Err(Error::invalid_param(format!(
                "sample ({}, {}) out of bounds ({}x{})",
                x, y, self.image_width, self.image_height
            )));
        }
        let tx = x / self.tile_size;
        let ty = y / self.tile_size;
        let coord = TileCoord::new(
            self.mip_level,
            tx,
            ty,
            self.tile_size,
            self.image_width,
            self.image_height,
        );
        let tile = self
            .read_tile(coord)?
            .ok_or_else(|| Error::invalid_param(format!("Tile ({}, {}) not stored", tx, ty)))?;
        let local_x = x - tile.coord.px;
        let local_y = y - tile.coord.py;
        let idx = (local_y * tile.coord.width + local_x) as usize;
        Ok(tile.data[idx])
    }

    pub fn has(&self, tile: &TileCoord) -> bool {
        self.tile_path(tile).exists()
    }

    /// Deletes ALL files (called on tab close).
    pub fn destroy(&self) -> Result<(), Error> {
        if self.base_dir.exists() {
            std::fs::remove_dir_all(&self.base_dir).map_err(Error::Io)
        } else {
            Ok(())
        }
    }
}

impl Clone for TileStore {
    fn clone(&self) -> Self {
        Self {
            base_dir: self.base_dir.clone(),
            tile_size: self.tile_size,
            image_width: self.image_width,
            image_height: self.image_height,
            mip_level: self.mip_level,
            hot_cache: RwLock::new(LruCache::new(self.hot_cache.read().cap())),
            auto_destroy: false,
        }
    }
}

impl Drop for TileStore {
    fn drop(&mut self) {
        if self.auto_destroy {
            let _ = self.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Rgba;
    use half::f16;

    fn make_coord(tx: u32, ty: u32, tile_size: u32, w: u32, h: u32) -> TileCoord {
        TileCoord::new(0, tx, ty, tile_size, w, h)
    }

    fn test_dir(id: &uuid::Uuid) -> PathBuf {
        std::env::temp_dir().join("pixors").join(id.to_string())
    }

    #[test]
    fn test_write_read_roundtrip() {
        let id = uuid::Uuid::new_v4();
        let store = TileStore::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = make_coord(0, 0, 256, 512, 512);
        let pixels: Vec<Rgba<f16>> = (0..256 * 256)
            .map(|i| {
                let v = f16::from_f32((i % 256) as f32 / 255.0);
                Rgba::new(v, v, v, f16::ONE)
            })
            .collect();
        let tile = Tile::new(coord, pixels.clone());

        store.write_tile_blocking(&tile).unwrap();
        let read_back = store.read_tile(coord).unwrap().unwrap();
        assert_eq!(read_back.data.len(), pixels.len());
        assert_eq!(read_back.data[0].r, pixels[0].r);
    }

    #[test]
    fn test_read_nonexistent() {
        let id = uuid::Uuid::new_v4();
        let store = TileStore::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = make_coord(99, 99, 256, 512, 512);
        assert!(store.read_tile(coord).unwrap().is_none());
    }

    #[test]
    fn test_sample() {
        let id = uuid::Uuid::new_v4();
        let store = TileStore::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = make_coord(0, 0, 256, 512, 512);
        let pixels = vec![
            Rgba::new(f16::from_f32(0.5), f16::from_f32(0.3), f16::from_f32(0.2), f16::ONE);
            256 * 256
        ];
        let tile = Tile::new(coord, pixels);
        store.write_tile_blocking(&tile).unwrap();

        let px = store.sample(10, 10).unwrap();
        assert!((px.r.to_f32() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_sample_out_of_bounds() {
        let id = uuid::Uuid::new_v4();
        let store = TileStore::new(test_dir(&id), 256, 512, 512).unwrap();
        assert!(store.sample(600, 600).is_err());
    }

    #[test]
    fn test_hot_cache_hit() {
        let id = uuid::Uuid::new_v4();
        let store = TileStore::new(test_dir(&id), 256, 512, 512).unwrap();
        let coord = make_coord(0, 0, 256, 512, 512);
        let pixels =
            vec![Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ONE); 256 * 256];
        let tile = Tile::new(coord, pixels);
        store.write_tile_blocking(&tile).unwrap();

        // First read populates cache
        let _ = store.read_tile(coord).unwrap();
        // Second read hits cache
        let t = store.read_tile(coord).unwrap().unwrap();
        assert_eq!(t.data.len(), 256 * 256);
    }
}
