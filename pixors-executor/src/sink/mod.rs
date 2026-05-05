pub mod cache_writer;
pub mod png_encoder;
pub mod tile_sink;
pub mod viewport;
pub mod viewport_cache_sink;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::sink::cache_writer::CacheWriter;
use crate::sink::png_encoder::PngEncoder;
use crate::sink::tile_sink::TileSink;
use crate::sink::viewport::ViewportSink;
use crate::sink::viewport_cache_sink::ViewportCacheSink;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SinkNode {
    Viewport(ViewportSink),
    TileSink(TileSink),
    PngEncoder(PngEncoder),
    CacheWriter(CacheWriter),
    ViewportCacheSink(ViewportCacheSink),
}

delegate_stage!(SinkNode, Viewport, TileSink, PngEncoder, CacheWriter, ViewportCacheSink);
