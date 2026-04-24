//! Async, tile-level image decoding.

use crate::error::Error;
use crate::storage::TileStore;
use async_trait::async_trait;
use std::path::Path;
use uuid::Uuid;

/// Async, tile-level image decoder. Implementations are format-specific.
#[async_trait]
pub trait ImageSource: Send + Sync {
    /// Image dimensions (available after open, before any tile decode).
    fn dimensions(&self) -> (u32, u32);

    /// Stream entire image into TileStore, one band at a time.
    /// Implementations should do a single pass through source data,
    /// accumulating tiles into bands for cache-friendly operation.
    async fn stream_to_store(&self, tile_size: u32, store: &TileStore, tab_id: Uuid)
        -> Result<(), Error>;
}

/// PNG implementation of `ImageSource`.
pub struct PngSource {
    width: u32,
    height: u32,
    path: std::path::PathBuf,
    color_space: crate::color::ColorSpace,
    alpha_mode: crate::image::AlphaMode,
}

impl PngSource {
    /// Opens a PNG file and reads its metadata.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path_buf = path.as_ref().to_path_buf();
        let path_for_blocking = path_buf.clone();
        // Read dimensions and color metadata via blocking task
        let (width, height, color_space, alpha_mode) = tokio::task::spawn_blocking(move || {
            crate::io::png::read_png_metadata(&path_for_blocking)
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

        Ok(Self {
            width,
            height,
            path: path_buf,
            color_space,
            alpha_mode,
        })
    }
}

#[async_trait]
impl ImageSource for PngSource {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    async fn stream_to_store(&self, tile_size: u32, store: &TileStore, _tab_id: Uuid) -> Result<(), Error> {
        let path = self.path.clone();
        let color_space = self.color_space;
        let alpha_mode = self.alpha_mode;
        let width = self.width;
        let height = self.height;
        let tile_size = tile_size.max(1);

        tokio::task::block_in_place(move || {
            crate::io::png::stream_png_to_tiles_sync(
                &path,
                width,
                height,
                tile_size,
                color_space,
                alpha_mode,
                store,
            )
        })
    }
}
