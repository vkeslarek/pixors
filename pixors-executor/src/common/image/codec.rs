use std::path::Path;
use crate::common::image::{ImageDescriptor, PageInfo};
use crate::error::Error;
use crate::graph::item::Item;

pub trait ImageDecoder: Send + Sync {
    fn probe(&self, path: &Path) -> Result<bool, Error>;
    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error>;
    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error>;
}

pub trait PageStream: Send {
    fn page_info(&self) -> &PageInfo;
    fn drain(&mut self, max_items: usize) -> Result<Vec<Item>, Error>;
}
