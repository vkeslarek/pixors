pub mod cache_reader;
pub mod image_stream;
pub mod viewport_cache_source;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::source::cache_reader::CacheReader;
use crate::source::image_stream::ImageStreamSource;
use crate::source::viewport_cache_source::ViewportCacheSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceNode {
    ImageStream(ImageStreamSource),
    CacheReader(CacheReader),
    ViewportCacheSource(ViewportCacheSource),
}

delegate_stage!(SourceNode, ImageStream, CacheReader, ViewportCacheSource);

impl From<ImageStreamSource> for SourceNode {
    fn from(v: ImageStreamSource) -> Self {
        Self::ImageStream(v)
    }
}

impl From<CacheReader> for SourceNode {
    fn from(v: CacheReader) -> Self {
        Self::CacheReader(v)
    }
}

impl From<ViewportCacheSource> for SourceNode {
    fn from(v: ViewportCacheSource) -> Self {
        Self::ViewportCacheSource(v)
    }
}
