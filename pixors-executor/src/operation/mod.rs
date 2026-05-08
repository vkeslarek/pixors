pub mod blur;
pub mod color;
pub mod compose;
pub mod mip_downsample;
pub mod mip_filter;
pub mod transfer;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::operation::blur::Blur;
use crate::operation::color::ColorConvert;
use crate::operation::compose::Compose;
use crate::operation::mip_downsample::MipDownsample;
use crate::operation::mip_filter::MipFilter;
use crate::operation::transfer::download::Download;
use crate::operation::transfer::upload::Upload;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationNode {
    Blur(Blur),
    ColorConvert(ColorConvert),
    Compose(Compose),
    MipDownsample(MipDownsample),
    MipFilter(MipFilter),
    Upload(Upload),
    Download(Download),
}

delegate_stage!(
    OperationNode,
    Blur,
    ColorConvert,
    Compose,
    MipDownsample,
    MipFilter,
    Upload,
    Download
);

impl From<Blur> for OperationNode {
    fn from(v: Blur) -> Self {
        Self::Blur(v)
    }
}

impl From<ColorConvert> for OperationNode {
    fn from(v: ColorConvert) -> Self {
        Self::ColorConvert(v)
    }
}

impl From<Compose> for OperationNode {
    fn from(v: Compose) -> Self {
        Self::Compose(v)
    }
}

impl From<MipDownsample> for OperationNode {
    fn from(v: MipDownsample) -> Self {
        Self::MipDownsample(v)
    }
}

impl From<MipFilter> for OperationNode {
    fn from(v: MipFilter) -> Self {
        Self::MipFilter(v)
    }
}

impl From<Upload> for OperationNode {
    fn from(v: Upload) -> Self {
        Self::Upload(v)
    }
}

impl From<Download> for OperationNode {
    fn from(v: Download) -> Self {
        Self::Download(v)
    }
}
