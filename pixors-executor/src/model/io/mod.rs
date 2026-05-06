//! I/O traits + format implementations.

use std::path::Path;

use crate::error::Error;
use crate::graph::item::Item;
use crate::model::image::desc::{ImageDescriptor, PageInfo};

pub mod png;
pub mod tiff;

pub trait ImageDecoder: Send + Sync {
    fn probe(&self, path: &Path) -> Result<bool, Error>;
    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error>;
    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error>;
}

pub trait PageStream: Send {
    fn page_info(&self) -> &PageInfo;
    fn drain(&mut self, max_items: usize) -> Result<Vec<Item>, Error>;
}
