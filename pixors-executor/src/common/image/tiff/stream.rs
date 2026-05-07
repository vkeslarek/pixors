use crate::data::buffer::Buffer;
use crate::data::scanline::ScanLine;
use crate::error::Error;
use crate::graph::item::Item;
use crate::common::pixel::PixelFormat;
use crate::common::pixel::meta::PixelMeta;

use ::tiff as tiff;

use super::super::codec::PageStream;
use super::super::*;

pub struct TiffPageStream {
    page_info: PageInfo,
    image_data: tiff::decoder::DecodingResult,
    color_type: tiff::ColorType,
    pixel_format: PixelFormat,
    width: u32,
    height: u32,
    row: u32,
    done: bool,
}

impl TiffPageStream {
    pub fn new(
        page_info: PageInfo,
        image_data: tiff::decoder::DecodingResult,
        color_type: tiff::ColorType,
        pixel_format: PixelFormat,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            page_info,
            image_data,
            color_type,
            pixel_format,
            width,
            height,
            row: 0,
            done: false,
        }
    }
}

impl PageStream for TiffPageStream {
    fn page_info(&self) -> &PageInfo {
        &self.page_info
    }

    fn drain(&mut self, max_items: usize) -> Result<Vec<Item>, Error> {
        if self.done {
            return Ok(vec![]);
        }
        let remaining = self.height.saturating_sub(self.row) as usize;
        let count = max_items.min(remaining);
        let mut items = Vec::with_capacity(count);
        let meta = PixelMeta::new(
            self.pixel_format,
            self.page_info.color_space,
            self.page_info.alpha_policy,
        );

        for _ in 0..count {
            let raw = tiff_row_bytes(&self.image_data, self.row, self.width, self.color_type)?;
            items.push(Item::ScanLine(ScanLine::new(
                0,
                self.row,
                self.width,
                meta,
                Buffer::cpu(raw),
            )));
            self.row += 1;
        }

        if self.row >= self.height {
            self.done = true;
        }
        Ok(items)
    }
}

pub fn tiff_pixel_format(
    result: &tiff::decoder::DecodingResult,
    ct: tiff::ColorType,
) -> PixelFormat {
    match result {
        tiff::decoder::DecodingResult::U8(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::Gray8,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayA8,
            tiff::ColorType::RGB(_) => PixelFormat::Rgb8,
            tiff::ColorType::RGBA(_) => PixelFormat::Rgba8,
            tiff::ColorType::CMYK(_) => PixelFormat::Cmyk8,
            tiff::ColorType::YCbCr(_) => PixelFormat::YCbCr8,
            _ => {
                tracing::warn!("Unsupported U8 TIFF color: {:?}, falling back to Rgba8", ct);
                PixelFormat::Rgba8
            }
        },
        tiff::decoder::DecodingResult::U16(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::Gray16,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayA16,
            tiff::ColorType::RGB(_) => PixelFormat::Rgb16,
            tiff::ColorType::RGBA(_) => PixelFormat::Rgba16,
            _ => {
                tracing::warn!("Unsupported U16 TIFF color: {:?}, falling back to Rgba16", ct);
                PixelFormat::Rgba16
            }
        },
        tiff::decoder::DecodingResult::F32(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF32,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF32,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF32,
            _ => {
                tracing::warn!("Unsupported F32 TIFF color: {:?}, falling back to RgbaF32", ct);
                PixelFormat::RgbaF32
            }
        },
        tiff::decoder::DecodingResult::U32(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF32,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF32,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF32,
            _ => {
                tracing::warn!("Unsupported U32 TIFF color: {:?}, falling back to RgbaF32", ct);
                PixelFormat::RgbaF32
            }
        },
        _ => {
            tracing::warn!("Unknown TIFF DecodingResult, falling back to Rgba8");
            PixelFormat::Rgba8
        }
    }
}

pub fn tiff_row_bytes(
    result: &tiff::decoder::DecodingResult,
    row: u32,
    width: u32,
    ct: tiff::ColorType,
) -> Result<Vec<u8>, Error> {
    let w = width as usize;
    let spp = ct.num_samples() as usize;
    let row_start = row as usize * w * spp;
    let row_end = row_start + w * spp;

    match result {
        tiff::decoder::DecodingResult::U8(data) => Ok(data[row_start..row_end].to_vec()),
        tiff::decoder::DecodingResult::U16(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::U32(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::F32(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::F16(data) => Ok(data[row_start..row_end]
            .iter()
            .flat_map(|v| v.to_f32().to_ne_bytes())
            .collect()),
        _ => Err(Error::unsupported_sample_type(format!(
            "Unsupported TIFF sample type for row decode: {:?}",
            result
        ))),
    }
}
