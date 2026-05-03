pub mod blur;
pub mod color;
pub mod composition;
pub mod transfer;
mod data;

use serde::{Deserialize, Serialize};

use crate::stage::{CpuKernel, GpuKernelDescriptor, PortSpec, Stage, StageHints};

use blur::Blur;
use color::ColorConvert;
pub use data::NeighborhoodAgg;
pub use data::ScanLineAccumulator;
pub use data::TileToScanline;
use transfer::Download;
use transfer::Upload;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationNode {
    Blur(Blur),
    ColorConvert(ColorConvert),
    ScanLineAccumulator(ScanLineAccumulator),
    NeighborhoodAgg(NeighborhoodAgg),
    TileToScanline(TileToScanline),
    Upload(Upload),
    Download(Download),
}

impl Stage for OperationNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::Blur(s) => s.kind(),
            Self::ColorConvert(s) => s.kind(),
            Self::ScanLineAccumulator(s) => s.kind(),
            Self::NeighborhoodAgg(s) => s.kind(),
            Self::TileToScanline(s) => s.kind(),
            Self::Upload(s) => s.kind(),
            Self::Download(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::Blur(s) => s.ports(),
            Self::ColorConvert(s) => s.ports(),
            Self::ScanLineAccumulator(s) => s.ports(),
            Self::NeighborhoodAgg(s) => s.ports(),
            Self::TileToScanline(s) => s.ports(),
            Self::Upload(s) => s.ports(),
            Self::Download(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::Blur(s) => s.hints(),
            Self::ColorConvert(s) => s.hints(),
            Self::ScanLineAccumulator(s) => s.hints(),
            Self::NeighborhoodAgg(s) => s.hints(),
            Self::TileToScanline(s) => s.hints(),
            Self::Upload(s) => s.hints(),
            Self::Download(s) => s.hints(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::Blur(s) => s.cpu_kernel(),
            Self::ColorConvert(s) => s.cpu_kernel(),
            Self::ScanLineAccumulator(s) => s.cpu_kernel(),
            Self::NeighborhoodAgg(s) => s.cpu_kernel(),
            Self::TileToScanline(s) => s.cpu_kernel(),
            Self::Upload(s) => s.cpu_kernel(),
            Self::Download(s) => s.cpu_kernel(),
        }
    }

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        match self {
            Self::Blur(s) => s.gpu_kernel_descriptor(),
            _ => None,
        }
    }
}
