pub mod cache_writer;
pub mod png_encoder;
pub mod tile_sink;

use serde::{Deserialize, Serialize};

use crate::stage::{CpuKernel, GpuKernelDescriptor, PortSpec, Stage, StageHints};

use cache_writer::CacheWriter;
use png_encoder::PngEncoder;
use tile_sink::TileSink;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SinkNode {
    TileSink(TileSink),
    PngEncoder(PngEncoder),
    CacheWriter(CacheWriter),
}

impl Stage for SinkNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::TileSink(s) => s.kind(),
            Self::PngEncoder(s) => s.kind(),
            Self::CacheWriter(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::TileSink(s) => s.ports(),
            Self::PngEncoder(s) => s.ports(),
            Self::CacheWriter(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::TileSink(s) => s.hints(),
            Self::PngEncoder(s) => s.hints(),
            Self::CacheWriter(s) => s.hints(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::TileSink(s) => s.cpu_kernel(),
            Self::PngEncoder(s) => s.cpu_kernel(),
            Self::CacheWriter(s) => s.cpu_kernel(),
        }
    }

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        None
    }
}
