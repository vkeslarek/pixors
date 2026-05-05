use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Gray8,
    GrayA8,
    Rgb8,
    Rgba8,
    Gray16,
    GrayA16,
    Rgb16,
    Rgba16,
    GrayF16,
    GrayAF16,
    RgbF16,
    RgbaF16,
    GrayF32,
    GrayAF32,
    RgbF32,
    RgbaF32,
    Argb32,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Gray8 => 1,
            PixelFormat::GrayA8 => 2,
            PixelFormat::Rgb8 => 3,
            PixelFormat::Rgba8 => 4,
            PixelFormat::Gray16 => 2,
            PixelFormat::GrayA16 => 4,
            PixelFormat::Rgb16 => 6,
            PixelFormat::Rgba16 => 8,
            PixelFormat::GrayF16 => 2,
            PixelFormat::GrayAF16 => 4,
            PixelFormat::RgbF16 => 6,
            PixelFormat::RgbaF16 => 8,
            PixelFormat::GrayF32 => 4,
            PixelFormat::GrayAF32 => 8,
            PixelFormat::RgbF32 => 12,
            PixelFormat::RgbaF32 => 16,
            PixelFormat::Argb32 => 4,
        }
    }
}
