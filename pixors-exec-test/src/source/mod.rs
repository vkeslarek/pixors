pub mod cache_reader;
pub mod file_decoder;

use serde::{Deserialize, Serialize};

use crate::stage::{CpuKernel, GpuKernelDescriptor, PortSpec, Stage, StageHints};

use cache_reader::CacheReader;
use file_decoder::FileDecoder;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceNode {
    FileDecoder(FileDecoder),
    CacheReader(CacheReader),
}

impl Stage for SourceNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::FileDecoder(s) => s.kind(),
            Self::CacheReader(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::FileDecoder(s) => s.ports(),
            Self::CacheReader(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::FileDecoder(s) => s.hints(),
            Self::CacheReader(s) => s.hints(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::FileDecoder(s) => s.cpu_kernel(),
            Self::CacheReader(s) => s.cpu_kernel(),
        }
    }

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        None
    }
}
