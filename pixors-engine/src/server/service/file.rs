use crate::error::Error;
use crate::image::TypedImage;
use crate::io::png::load_png;
use crate::pixel::Rgba;
use crate::storage::PngSource;
use half::f16;
use std::path::Path;

/// Service responsible for handling file IO and decoding/encoding operations.
#[derive(Debug, Default)]
pub struct FileService;

impl FileService {
    pub fn new() -> Self {
        Self
    }

    /// Opens an image from the filesystem and converts it to the working color space.
    pub async fn open_image(&self, path: &str) -> Result<TypedImage<Rgba<f16>>, Error> {
        // In a real async scenario, file IO should be spawned on blocking threads
        let path_buf = path.to_string();
        
        tokio::task::spawn_blocking(move || {
            let raw = load_png(Path::new(&path_buf))?;
            crate::convert::convert_raw_to_typed(raw)
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
    }

    /// Opens an image as a tile-level decoder (lazy loading).
    pub async fn open_image_source(&self, path: impl AsRef<Path>) -> Result<PngSource, Error> {
        PngSource::open(path).await
    }
}
