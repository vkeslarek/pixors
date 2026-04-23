//! Async, tile-level image decoding.

use crate::error::Error;
use crate::image::TypedImage;
use crate::pixel::Rgba;
use async_trait::async_trait;
use half::f16;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Async, tile-level image decoder. Implementations are format-specific.
#[async_trait]
pub trait ImageSource: Send + Sync {
    /// Image dimensions (available after open, before any tile decode).
    fn dimensions(&self) -> (u32, u32);

    /// Decode a single tile region to ACEScg premul f16.
    /// The implementation may read only the relevant bytes from disk.
    async fn decode_tile(&self, x: u32, y: u32, w: u32, h: u32)
        -> Result<Vec<Rgba<f16>>, Error>;
}

/// PNG implementation of `ImageSource`.
pub struct PngSource {
    width: u32,
    height: u32,
    path: std::path::PathBuf,
    decoded_cache: OnceCell<Arc<TypedImage<Rgba<f16>>>>,
}

impl PngSource {
    /// Opens a PNG file and reads its metadata.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path_buf = path.as_ref().to_path_buf();
        let path_for_blocking = path_buf.clone();
        // Read dimensions via blocking task
        let (width, height) = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&path_for_blocking)?;
            let decoder = png::Decoder::new(BufReader::new(file));
            let reader = decoder.read_info()?;
            let info = reader.info();
            Ok::<_, Error>((info.width, info.height))
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;

        Ok(Self {
            width,
            height,
            path: path_buf,
            decoded_cache: OnceCell::new(),
        })
    }

    async fn decoded_image(&self) -> Result<Arc<TypedImage<Rgba<f16>>>, Error> {
        let path = self.path.clone();
        let image = self
            .decoded_cache
            .get_or_try_init(|| async move {
                let typed = tokio::task::spawn_blocking(move || {
                    let raw = crate::io::png::load_png(&path)?;
                    crate::convert::convert_raw_to_typed(raw)
                })
                .await
                .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;
                Ok::<Arc<TypedImage<Rgba<f16>>>, Error>(Arc::new(typed))
            })
            .await?;

        Ok(Arc::clone(image))
    }
}

#[async_trait]
impl ImageSource for PngSource {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    async fn decode_tile(&self, x: u32, y: u32, w: u32, h: u32) -> Result<Vec<Rgba<f16>>, Error> {
        let typed = self.decoded_image().await?;

        if x + w > typed.width || y + h > typed.height {
            return Err(Error::invalid_param("Requested tile is out of image bounds"));
        }

        let mut tile_pixels = Vec::with_capacity((w * h) as usize);
        let image_width = typed.width as usize;
        let x = x as usize;
        let y = y as usize;
        let w = w as usize;
        let h = h as usize;

        for row in 0..h {
            let row_start = (y + row) * image_width + x;
            let row_end = row_start + w;
            tile_pixels.extend_from_slice(&typed.pixels[row_start..row_end]);
        }

        Ok(tile_pixels)
    }
}
