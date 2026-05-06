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
        self.sample_bytes() * self.channel_count()
    }

    pub fn channel_count(self) -> usize {
        match self {
            PixelFormat::Gray8 | PixelFormat::Gray16 | PixelFormat::GrayF16
            | PixelFormat::GrayF32 => 1,
            PixelFormat::GrayA8 | PixelFormat::GrayA16 | PixelFormat::GrayAF16
            | PixelFormat::GrayAF32 => 2,
            PixelFormat::Rgb8 | PixelFormat::Rgb16 | PixelFormat::RgbF16
            | PixelFormat::RgbF32 => 3,
            PixelFormat::Rgba8 | PixelFormat::Rgba16 | PixelFormat::RgbaF16
            | PixelFormat::RgbaF32 | PixelFormat::Argb32 => 4,
        }
    }

    pub fn sample_bytes(self) -> usize {
        match self {
            PixelFormat::Gray8 | PixelFormat::GrayA8 | PixelFormat::Rgb8
            | PixelFormat::Rgba8 | PixelFormat::Argb32 => 1,
            PixelFormat::Gray16 | PixelFormat::GrayA16 | PixelFormat::Rgb16
            | PixelFormat::Rgba16 | PixelFormat::GrayF16 | PixelFormat::GrayAF16
            | PixelFormat::RgbF16 | PixelFormat::RgbaF16 => 2,
            PixelFormat::GrayF32 | PixelFormat::GrayAF32 | PixelFormat::RgbF32
            | PixelFormat::RgbaF32 => 4,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self,
            PixelFormat::GrayF16 | PixelFormat::GrayAF16 | PixelFormat::RgbF16 | PixelFormat::RgbaF16
            | PixelFormat::GrayF32 | PixelFormat::GrayAF32 | PixelFormat::RgbF32 | PixelFormat::RgbaF32
        )
    }

    pub fn is_integer(self) -> bool {
        !self.is_float()
    }

    pub fn max_value(self) -> f32 {
        match self {
            PixelFormat::Gray8 | PixelFormat::GrayA8 | PixelFormat::Rgb8
            | PixelFormat::Rgba8 | PixelFormat::Argb32 => 255.0,
            PixelFormat::Gray16 | PixelFormat::GrayA16 | PixelFormat::Rgb16
            | PixelFormat::Rgba16 => 65535.0,
            _ => 1.0,
        }
    }

    pub fn scale_to_f32(self) -> f32 {
        match self {
            PixelFormat::Gray8 | PixelFormat::GrayA8 | PixelFormat::Rgb8
            | PixelFormat::Rgba8 | PixelFormat::Argb32 => 1.0 / 255.0,
            PixelFormat::Gray16 | PixelFormat::GrayA16 | PixelFormat::Rgb16
            | PixelFormat::Rgba16 => 1.0 / 65535.0,
            _ => 1.0,
        }
    }

    pub fn scale_from_f32(self) -> f32 {
        match self {
            PixelFormat::Gray8 | PixelFormat::GrayA8 | PixelFormat::Rgb8
            | PixelFormat::Rgba8 | PixelFormat::Argb32 => 255.0,
            PixelFormat::Gray16 | PixelFormat::GrayA16 | PixelFormat::Rgb16
            | PixelFormat::Rgba16 => 65535.0,
            _ => 1.0,
        }
    }
}
