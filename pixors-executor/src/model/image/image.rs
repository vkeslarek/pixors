use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::Error;
use crate::model::image::decoder::{ImageDecoder, PageStream};
use crate::model::image::desc::ImageDesc;

pub struct Image {
    pub desc: ImageDesc,
    decoder: Arc<dyn ImageDecoder>,
    path: PathBuf,
}

impl Image {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        if let Ok(desc) = crate::model::io::png::PngDecoder.decode(path) {
            return Ok(Self {
                desc,
                decoder: Arc::new(crate::model::io::png::PngDecoder),
                path: path.to_path_buf(),
            });
        }
        if let Ok(desc) = crate::model::io::tiff::TiffDecoder.decode(path) {
            return Ok(Self {
                desc,
                decoder: Arc::new(crate::model::io::tiff::TiffDecoder),
                path: path.to_path_buf(),
            });
        }
        Err(Error::internal("unsupported image format"))
    }

    pub fn open_page(&self, page: usize) -> Result<Box<dyn PageStream>, Error> {
        self.decoder.open_stream(&self.path, page)
    }

    pub fn page_count(&self) -> usize {
        self.desc.pages.len()
    }
}

impl Clone for Image {
    fn clone(&self) -> Self {
        Self {
            desc: self.desc.clone(),
            decoder: Arc::clone(&self.decoder),
            path: self.path.clone(),
        }
    }
}
