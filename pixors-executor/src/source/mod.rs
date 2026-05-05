pub mod cache_reader;
pub mod file_decoder;
pub mod image_file_source;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::source::cache_reader::CacheReader;
use crate::source::file_decoder::FileDecoder;
use crate::source::image_file_source::ImageFileSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceNode {
    ImageFile(ImageFileSource),
    FileDecoder(FileDecoder),
    CacheReader(CacheReader),
}

delegate_stage!(SourceNode, ImageFile, FileDecoder, CacheReader);
