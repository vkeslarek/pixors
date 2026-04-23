//! Disk-backed tile storage.

use crate::error::Error;
use crate::image::Tile;
use crate::pixel::Rgba;
use half::f16;
use std::fs;
use std::path::PathBuf;

/// Persists decoded tiles as raw f16 blobs in a temp directory.
/// Each tile is a file: `{tab_tmp}/{tile_x}_{tile_y}.raw`
#[derive(Debug)]
pub struct TileStore {
    base_dir: PathBuf,           // e.g. /tmp/pixors-{tab_id}/
}

impl TileStore {
    /// Creates a new tile store for a given tab.
    /// The temporary directory is created immediately.
    pub fn new(tab_id: &uuid::Uuid, _tile_size: u32, _image_width: u32, _image_height: u32) -> Result<Self, Error> {
        let base_dir = std::env::temp_dir()
            .join("pixors")
            .join(tab_id.to_string());
        fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Creates a tile store in a subdirectory under the tab's temp directory.
    /// Used for MIP levels to avoid file collisions with the base level.
    pub fn new_with_subdir(tab_id: &uuid::Uuid, subdir: &str) -> Result<Self, Error> {
        let base_dir = std::env::temp_dir()
            .join("pixors")
            .join(tab_id.to_string())
            .join(subdir);
        fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Returns the file path for a given tile.
    fn tile_path(&self, tile: &Tile) -> PathBuf {
        self.base_dir.join(format!("{}_{}.raw", tile.x, tile.y))
    }

    /// Writes a decoded tile to disk. Called lazily on first access.
    pub async fn put(&self, tile: &Tile, data: &[Rgba<f16>]) -> Result<(), Error> {
        let path = self.tile_path(tile);
        let data_bytes = bytemuck::cast_slice(data).to_vec();
        tokio::task::spawn_blocking(move || fs::write(path, data_bytes))
            .await
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
            .map_err(Error::Io)
    }

    /// Reads a tile from disk. Returns None if not yet decoded.
    pub async fn get(&self, tile: &Tile) -> Result<Option<Vec<Rgba<f16>>>, Error> {
        let path = self.tile_path(tile);
        if !path.exists() {
            return Ok(None);
        }
        let tile_size = (tile.width * tile.height) as usize;
        let bytes = tokio::task::spawn_blocking(move || fs::read(path))
            .await
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
            .map_err(Error::Io)?;
        if bytes.len() != tile_size * std::mem::size_of::<Rgba<f16>>() {
            return Err(Error::invalid_param("Tile file size mismatch"));
        }
        let pixels = bytemuck::cast_slice(&bytes).to_vec();
        Ok(Some(pixels))
    }

    /// Checks if a tile has been decoded and stored.
    pub fn has(&self, tile: &Tile) -> bool {
        self.tile_path(tile).exists()
    }

    /// Deletes a specific tile file from disk.
    pub async fn delete_tile(&self, tile: &Tile) -> Result<(), Error> {
        let path = self.tile_path(tile);
        if !path.exists() {
            return Ok(());
        }
        tokio::task::spawn_blocking(move || fs::remove_file(path))
            .await
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
            .map_err(Error::Io)
    }

    /// Deletes multiple tile files from disk.
    pub async fn delete_tiles(&self, tiles: &[Tile]) -> Result<(), Error> {
        let paths: Vec<PathBuf> = tiles.iter().map(|t| self.tile_path(t)).collect();
        tokio::task::spawn_blocking(move || {
            for path in paths {
                if path.exists() {
                    let _ = fs::remove_file(path);
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
    }

    /// Deletes ALL files (called on tab close).
    pub fn destroy(&self) -> Result<(), Error> {
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir).map_err(Error::Io)
        } else {
            Ok(())
        }
    }
}

impl Drop for TileStore {
    fn drop(&mut self) {
        // Note: we might want to keep tiles for a while, but for now clean up.
        let _ = self.destroy();
    }
}