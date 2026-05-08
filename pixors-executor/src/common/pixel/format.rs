use serde::{Deserialize, Serialize};

use crate::common::color::model::ColorModelTransform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Gray8,
    GrayA8,
    Rgb8,
    Rgba8,
    Cmyk8,
    CmykA8,
    YCbCr8,
    Lab8,
    Gray16,
    GrayA16,
    Rgb16,
    Rgba16,
    Cmyk16,
    CmykA16,
    Lab16,
    GrayF16,
    GrayAF16,
    RgbF16,
    RgbaF16,
    CmykF16,
    CmykAF16,
    YCbCrF16,
    GrayF32,
    GrayAF32,
    RgbF32,
    RgbaF32,
    CmykF32,
    CmykAF32,
    YCbCrF32,
    Argb32,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        self.sample_bytes() * self.channel_count()
    }

    pub fn channel_count(self) -> usize {
        match self {
            PixelFormat::Gray8
            | PixelFormat::Gray16
            | PixelFormat::GrayF16
            | PixelFormat::GrayF32 => 1,
            PixelFormat::GrayA8
            | PixelFormat::GrayA16
            | PixelFormat::GrayAF16
            | PixelFormat::GrayAF32 => 2,
            PixelFormat::Rgb8
            | PixelFormat::Rgb16
            | PixelFormat::RgbF16
            | PixelFormat::RgbF32
            | PixelFormat::YCbCr8
            | PixelFormat::YCbCrF16
            | PixelFormat::YCbCrF32
            | PixelFormat::Lab8
            | PixelFormat::Lab16 => 3,
            PixelFormat::Rgba8
            | PixelFormat::Rgba16
            | PixelFormat::RgbaF16
            | PixelFormat::RgbaF32
            | PixelFormat::Argb32
            | PixelFormat::Cmyk8
            | PixelFormat::Cmyk16
            | PixelFormat::CmykF16
            | PixelFormat::CmykF32 => 4,
            PixelFormat::CmykA8
            | PixelFormat::CmykA16
            | PixelFormat::CmykAF16
            | PixelFormat::CmykAF32 => 5,
        }
    }

    pub fn sample_bytes(self) -> usize {
        match self {
            PixelFormat::Gray8
            | PixelFormat::GrayA8
            | PixelFormat::Rgb8
            | PixelFormat::Rgba8
            | PixelFormat::Argb32
            | PixelFormat::Cmyk8
            | PixelFormat::CmykA8
            | PixelFormat::YCbCr8
            | PixelFormat::Lab8 => 1,
            PixelFormat::Gray16
            | PixelFormat::GrayA16
            | PixelFormat::Rgb16
            | PixelFormat::Rgba16
            | PixelFormat::GrayF16
            | PixelFormat::GrayAF16
            | PixelFormat::RgbF16
            | PixelFormat::RgbaF16
            | PixelFormat::Cmyk16
            | PixelFormat::CmykA16
            | PixelFormat::CmykF16
            | PixelFormat::CmykAF16
            | PixelFormat::YCbCrF16
            | PixelFormat::Lab16 => 2,
            PixelFormat::GrayF32
            | PixelFormat::GrayAF32
            | PixelFormat::RgbF32
            | PixelFormat::RgbaF32
            | PixelFormat::CmykF32
            | PixelFormat::CmykAF32
            | PixelFormat::YCbCrF32 => 4,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(
            self,
            PixelFormat::GrayF16
                | PixelFormat::GrayAF16
                | PixelFormat::RgbF16
                | PixelFormat::RgbaF16
                | PixelFormat::CmykF16
                | PixelFormat::CmykAF16
                | PixelFormat::YCbCrF16
                | PixelFormat::GrayF32
                | PixelFormat::GrayAF32
                | PixelFormat::RgbF32
                | PixelFormat::RgbaF32
                | PixelFormat::CmykF32
                | PixelFormat::CmykAF32
                | PixelFormat::YCbCrF32
        )
    }

    pub fn is_integer(self) -> bool {
        !self.is_float()
    }

    pub fn model_transform(self) -> ColorModelTransform {
        match self {
            PixelFormat::Cmyk8
            | PixelFormat::Cmyk16
            | PixelFormat::CmykF16
            | PixelFormat::CmykF32 => ColorModelTransform::CmykToRgb,
            PixelFormat::CmykA8
            | PixelFormat::CmykA16
            | PixelFormat::CmykAF16
            | PixelFormat::CmykAF32 => ColorModelTransform::CmykAToRgb,
            PixelFormat::YCbCr8 | PixelFormat::YCbCrF16 | PixelFormat::YCbCrF32 => {
                ColorModelTransform::YCbCrToRgb
            }
            PixelFormat::Lab8 | PixelFormat::Lab16 => ColorModelTransform::LabToRgb,
            _ => ColorModelTransform::None,
        }
    }

    pub fn max_value(self) -> f32 {
        match self {
            PixelFormat::Gray8
            | PixelFormat::GrayA8
            | PixelFormat::Rgb8
            | PixelFormat::Rgba8
            | PixelFormat::Argb32
            | PixelFormat::Cmyk8
            | PixelFormat::CmykA8
            | PixelFormat::YCbCr8
            | PixelFormat::Lab8 => 255.0,
            PixelFormat::Gray16
            | PixelFormat::GrayA16
            | PixelFormat::Rgb16
            | PixelFormat::Rgba16
            | PixelFormat::Cmyk16
            | PixelFormat::CmykA16
            | PixelFormat::Lab16 => 65535.0,
            _ => 1.0,
        }
    }

    pub fn scale_to_f32(self) -> f32 {
        match self {
            PixelFormat::Gray8
            | PixelFormat::GrayA8
            | PixelFormat::Rgb8
            | PixelFormat::Rgba8
            | PixelFormat::Argb32
            | PixelFormat::Cmyk8
            | PixelFormat::CmykA8
            | PixelFormat::YCbCr8
            | PixelFormat::Lab8 => 1.0 / 255.0,
            PixelFormat::Gray16
            | PixelFormat::GrayA16
            | PixelFormat::Rgb16
            | PixelFormat::Rgba16
            | PixelFormat::Cmyk16
            | PixelFormat::CmykA16
            | PixelFormat::Lab16 => 1.0 / 65535.0,
            _ => 1.0,
        }
    }

    pub fn scale_from_f32(self) -> f32 {
        match self {
            PixelFormat::Gray8
            | PixelFormat::GrayA8
            | PixelFormat::Rgb8
            | PixelFormat::Rgba8
            | PixelFormat::Argb32
            | PixelFormat::Cmyk8
            | PixelFormat::CmykA8
            | PixelFormat::YCbCr8
            | PixelFormat::Lab8 => 255.0,
            PixelFormat::Gray16
            | PixelFormat::GrayA16
            | PixelFormat::Rgb16
            | PixelFormat::Rgba16
            | PixelFormat::Cmyk16
            | PixelFormat::CmykA16
            | PixelFormat::Lab16 => 65535.0,
            _ => 1.0,
        }
    }
}
