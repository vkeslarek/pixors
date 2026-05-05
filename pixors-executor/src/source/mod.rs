pub mod cache_reader;
pub mod image_stream;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::source::cache_reader::CacheReader;
use crate::source::image_stream::ImageStreamSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceNode {
    ImageStream(ImageStreamSource),
    CacheReader(CacheReader),
}

delegate_stage!(SourceNode, ImageStream, CacheReader);
