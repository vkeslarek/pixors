pub mod blur;
pub mod color;
pub mod composition;
pub mod mip_downsample;
pub mod mip_filter;
pub mod transfer;

use serde::{Deserialize, Serialize};

use crate::data::Device;
use crate::stage::{CpuKernel, GpuKernelDescriptor, PortSpec, Stage, StageHints};

use blur::Blur;
use color::ColorConvert;
use mip_downsample::MipDownsample;
use mip_filter::MipFilter;
use transfer::Download;
use transfer::Upload;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationNode {
    Blur(Blur),
    ColorConvert(ColorConvert),
    MipDownsample(MipDownsample),
    MipFilter(MipFilter),
    Upload(Upload),
    Download(Download),
}

impl Stage for OperationNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::Blur(s) => s.kind(),
            Self::ColorConvert(s) => s.kind(),
            Self::MipDownsample(s) => s.kind(),
            Self::MipFilter(s) => s.kind(),
            Self::Upload(s) => s.kind(),
            Self::Download(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::Blur(s) => s.ports(),
            Self::ColorConvert(s) => s.ports(),
            Self::MipDownsample(s) => s.ports(),
            Self::MipFilter(s) => s.ports(),
            Self::Upload(s) => s.ports(),
            Self::Download(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::Blur(s) => s.hints(),
            Self::ColorConvert(s) => s.hints(),
            Self::MipDownsample(s) => s.hints(),
            Self::MipFilter(s) => s.hints(),
            Self::Upload(s) => s.hints(),
            Self::Download(s) => s.hints(),
        }
    }

    fn device(&self) -> Device {
        match self {
            Self::Blur(s) => s.device(),
            Self::ColorConvert(s) => s.device(),
            Self::MipDownsample(s) => s.device(),
            Self::MipFilter(s) => s.device(),
            Self::Upload(s) => s.device(),
            Self::Download(s) => s.device(),
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::Blur(s) => s.cpu_kernel(),
            Self::ColorConvert(s) => s.cpu_kernel(),
            Self::MipDownsample(s) => s.cpu_kernel(),
            Self::MipFilter(s) => s.cpu_kernel(),
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
