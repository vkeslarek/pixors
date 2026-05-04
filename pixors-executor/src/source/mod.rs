pub mod cache_reader;
pub mod file_decoder;
pub mod image_file_source;

use serde::{Deserialize, Serialize};

use crate::data::Device;
use crate::stage::{CpuKernel, GpuKernelDescriptor, PortSpec, Stage, StageHints};

use cache_reader::CacheReader;
use file_decoder::FileDecoder;
pub use image_file_source::ImageFileSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceNode {
    ImageFile(ImageFileSource),
    FileDecoder(FileDecoder),
    CacheReader(CacheReader),
}

impl Stage for SourceNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::ImageFile(s) => s.kind(),
            Self::FileDecoder(s) => s.kind(),
            Self::CacheReader(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::ImageFile(s) => s.ports(),
            Self::FileDecoder(s) => s.ports(),
            Self::CacheReader(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::ImageFile(s) => s.hints(),
            Self::FileDecoder(s) => s.hints(),
            Self::CacheReader(s) => s.hints(),
        }
    }

    fn device(&self) -> Device {
        match self {
            Self::ImageFile(s) => s.device(),
            Self::FileDecoder(s) => s.device(),
            Self::CacheReader(s) => s.device(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::ImageFile(s) => s.cpu_kernel(),
            Self::FileDecoder(s) => s.cpu_kernel(),
            Self::CacheReader(s) => s.cpu_kernel(),
        }
    }

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        None
    }
}
