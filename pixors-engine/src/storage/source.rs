//! Async, tile-level image decoding.

use crate::color::ColorSpace;
use crate::convert::tile_stream::convert_to_tiles;
use crate::error::Error;
use crate::io::ImageReader;
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
    /// `on_progress` is called with percent (0–100) after each band completes.
    async fn stream_to_store(&self, tile_size: u32, store: &TileStore, tab_id: Uuid, on_progress: Option<Box<dyn Fn(u8) + Send>>)
        -> Result<(), Error>;
}

/// Generic image source backed by any `ImageReader` implementation.
pub struct FormatSource {
    width: u32,
    height: u32,
    path: std::path::PathBuf,
    color_space: crate::color::ColorSpace,
    #[allow(dead_code)]
    alpha_mode: crate::image::AlphaMode,
    reader: &'static dyn ImageReader,
}

impl FormatSource {
    /// Opens an image file using the first registered reader that can handle it.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path_buf = path.as_ref().to_path_buf();

        for reader in crate::io::all_readers() {
            if reader.can_handle(path.as_ref()) {
                let reader = *reader;
                let path_for_blocking = path_buf.clone();
                let (width, height, color_space, alpha_mode) = tokio::task::spawn_blocking(move || {
                    reader.read_metadata(&path_for_blocking)
                })
                .await
                .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

                return Ok(Self {
                    width,
                    height,
                    path: path_buf,
                    color_space,
                    alpha_mode,
                    reader,
                });
            }
        }

        Err(Error::unsupported_sample_type(format!(
            "No image format reader available for {}",
            path_buf.display()
        )))
    }
}

#[async_trait]
impl ImageSource for FormatSource {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    async fn stream_to_store(&self, tile_size: u32, store: &TileStore, _tab_id: Uuid, on_progress: Option<Box<dyn Fn(u8) + Send>>) -> Result<(), Error> {
        let reader = self.reader;
        let path = self.path.clone();
        let width = self.width;
        let height = self.height;
        let tile_size = tile_size.max(1);
        let color_space = self.color_space;

        tokio::task::block_in_place(move || {
            let buf = reader.load(&path)?;
            assert_eq!(buf.desc.width, width);
            assert_eq!(buf.desc.height, height);
            let conv = color_space.converter_to(ColorSpace::ACES_CG)?;
            convert_to_tiles(&conv, &buf, tile_size, store, on_progress.as_deref())
        })
    }
}
