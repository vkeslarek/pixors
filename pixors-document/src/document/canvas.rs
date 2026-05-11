use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::PixelFormat;
use serde::{Deserialize, Serialize};

/// Document-level canvas configuration.
/// Determines the working space for all color operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasInfo {
    pub width: u32,
    pub height: u32,
    pub working_color_space: ColorSpace,
    pub working_format: PixelFormat,
}

impl Default for CanvasInfo {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            working_color_space: ColorSpace::ACES_CG,
            working_format: PixelFormat::RgbaF16,
        }
    }
}
