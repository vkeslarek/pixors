pub mod blur;
pub mod color;
pub mod compose;
pub mod composition;
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
