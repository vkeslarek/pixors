use crate::model::color::space::ColorSpace;
use crate::model::pixel::{AlphaPolicy, PixelFormat};

#[derive(Debug, Clone, Copy)]
pub struct PixelMeta {
    pub format: PixelFormat,
    pub color_space: ColorSpace,
    pub alpha_policy: AlphaPolicy,
}

impl PixelMeta {
    pub fn new(format: PixelFormat, color_space: ColorSpace, alpha_policy: AlphaPolicy) -> Self {
        Self {
            format,
            color_space,
            alpha_policy,
        }
    }
}
