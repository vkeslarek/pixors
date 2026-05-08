pub mod cache_writer;
pub mod png_encoder;
pub mod png_encoder_v2;
pub mod tiff_encoder;
pub mod viewport;
pub mod viewport_cache_sink;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::sink::cache_writer::CacheWriter;
use crate::sink::png_encoder::PngEncoder;
use crate::sink::png_encoder_v2::PngEncoderV2;
use crate::sink::tiff_encoder::TiffEncoderStage;
use crate::sink::viewport::ViewportSink;
use crate::sink::viewport_cache_sink::ViewportCacheSink;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SinkNode {
    Viewport(ViewportSink),
    PngEncoder(PngEncoder),
    PngEncoderV2(PngEncoderV2),
    TiffEncoder(TiffEncoderStage),
    CacheWriter(CacheWriter),
    ViewportCacheSink(ViewportCacheSink),
}

delegate_stage!(
    SinkNode,
    Viewport,
    PngEncoder,
    PngEncoderV2,
    TiffEncoder,
    CacheWriter,
    ViewportCacheSink
);

impl From<ViewportSink> for SinkNode {
    fn from(v: ViewportSink) -> Self { Self::Viewport(v) }
}

impl From<PngEncoder> for SinkNode {
    fn from(v: PngEncoder) -> Self { Self::PngEncoder(v) }
}

impl From<PngEncoderV2> for SinkNode {
    fn from(v: PngEncoderV2) -> Self { Self::PngEncoderV2(v) }
}

impl From<TiffEncoderStage> for SinkNode {
    fn from(v: TiffEncoderStage) -> Self { Self::TiffEncoder(v) }
}

impl From<CacheWriter> for SinkNode {
    fn from(v: CacheWriter) -> Self { Self::CacheWriter(v) }
}

impl From<ViewportCacheSink> for SinkNode {
    fn from(v: ViewportCacheSink) -> Self { Self::ViewportCacheSink(v) }
}
