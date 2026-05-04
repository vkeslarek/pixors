pub mod cache_writer;
pub mod png_encoder;
pub mod tile_sink;
pub mod viewport;
pub mod viewport_cache_sink;

use serde::{Deserialize, Serialize};

use crate::data::Device;
use crate::stage::{CpuKernel, PortSpec, Stage, StageHints};

pub use cache_writer::CacheWriter;
use png_encoder::PngEncoder;
use tile_sink::TileSink;
use viewport::ViewportSink;
pub use viewport_cache_sink::ViewportCacheSink;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SinkNode {
    Viewport(ViewportSink),
    TileSink(TileSink),
    PngEncoder(PngEncoder),
    CacheWriter(CacheWriter),
    ViewportCacheSink(ViewportCacheSink),
}

impl Stage for SinkNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::Viewport(s) => s.kind(),
            Self::TileSink(s) => s.kind(),
            Self::PngEncoder(s) => s.kind(),
            Self::CacheWriter(s) => s.kind(),
            Self::ViewportCacheSink(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::Viewport(s) => s.ports(),
            Self::TileSink(s) => s.ports(),
            Self::PngEncoder(s) => s.ports(),
            Self::CacheWriter(s) => s.ports(),
            Self::ViewportCacheSink(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::Viewport(s) => s.hints(),
            Self::TileSink(s) => s.hints(),
            Self::PngEncoder(s) => s.hints(),
            Self::CacheWriter(s) => s.hints(),
            Self::ViewportCacheSink(s) => s.hints(),
        }
    }

    fn device(&self) -> Device {
        match self {
            Self::Viewport(s) => s.device(),
            Self::TileSink(s) => s.device(),
            Self::PngEncoder(s) => s.device(),
            Self::CacheWriter(s) => s.device(),
            Self::ViewportCacheSink(s) => s.device(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::Viewport(s) => s.cpu_kernel(),
            Self::TileSink(s) => s.cpu_kernel(),
            Self::PngEncoder(s) => s.cpu_kernel(),
            Self::CacheWriter(s) => s.cpu_kernel(),
            Self::ViewportCacheSink(s) => s.cpu_kernel(),
        }
    }

}
