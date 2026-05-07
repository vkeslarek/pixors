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
    planar: bool,
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
        planar: bool,
    ) -> Self {
        Self {
            page_info,
            image_data,
            color_type,
            pixel_format,
            width,
            height,
            planar,
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
            let raw = tiff_row_bytes(&self.image_data, self.row, self.width, self.height, self.color_type, self.planar)?;
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
            tiff::ColorType::CMYKA(_) => PixelFormat::CmykA8,
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
        tiff::decoder::DecodingResult::F16(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF16,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF16,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF16,
            _ => {
                tracing::warn!("Unsupported F16 TIFF color: {:?}, falling back to RgbaF16", ct);
                PixelFormat::RgbaF16
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
    height: u32,
    ct: tiff::ColorType,
    planar: bool,
) -> Result<Vec<u8>, Error> {
    let w = width as usize;
    let h = height as usize;
    let spp = ct.num_samples() as usize;
    match result {
        tiff::decoder::DecodingResult::U8(data) => {
            Ok(row_bytes_u8(data, row, w, h, spp, planar)?.to_vec())
        }
        tiff::decoder::DecodingResult::U16(data) => Ok(row_bytes_u16(data, row, w, h, spp, planar)?
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::U32(data) => Ok(row_bytes_u32(data, row, w, h, spp, planar)?
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::F32(data) => Ok(row_bytes_f32(data, row, w, h, spp, planar)?
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::F16(data) => Ok(row_bytes_f16(data, row, w, h, spp, planar)?
            .iter()
            .flat_map(|v| v.to_f32().to_ne_bytes())
            .collect()),
        _ => Err(Error::unsupported_sample_type(format!(
            "Unsupported TIFF sample type for row decode: {:?}",
            result
        ))),
    }
}

fn row_bytes_u8(data: &[u8], row: u32, w: usize, h: usize, spp: usize, planar: bool) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(w * spp);
    if planar {
        let plane_len = w * h;
        for ch in 0..spp {
            let start = ch * plane_len + row as usize * w;
            out.extend_from_slice(data.get(start..start + w).ok_or_else(|| {
                Error::internal("TIFF planar row out of bounds")
            })?);
        }
    } else {
        let start = row as usize * w * spp;
        out.extend_from_slice(data.get(start..start + w * spp).ok_or_else(|| {
            Error::internal("TIFF row out of bounds")
        })?);
    }
    Ok(out)
}

fn row_bytes_u16(data: &[u16], row: u32, w: usize, h: usize, spp: usize, planar: bool) -> Result<Vec<u16>, Error> {
    if planar { row_planar(data, row, w, h, spp) } else { row_interleaved(data, row, w, spp) }
}

fn row_bytes_u32(data: &[u32], row: u32, w: usize, h: usize, spp: usize, planar: bool) -> Result<Vec<u32>, Error> {
    if planar { row_planar(data, row, w, h, spp) } else { row_interleaved(data, row, w, spp) }
}

fn row_bytes_f32(data: &[f32], row: u32, w: usize, h: usize, spp: usize, planar: bool) -> Result<Vec<f32>, Error> {
    if planar { row_planar(data, row, w, h, spp) } else { row_interleaved(data, row, w, spp) }
}

fn row_bytes_f16(data: &[half::f16], row: u32, w: usize, h: usize, spp: usize, planar: bool) -> Result<Vec<half::f16>, Error> {
    if planar { row_planar(data, row, w, h, spp) } else { row_interleaved(data, row, w, spp) }
}

fn row_interleaved<T: Copy>(data: &[T], row: u32, w: usize, spp: usize) -> Result<Vec<T>, Error> {
    let start = row as usize * w * spp;
    data.get(start..start + w * spp)
        .map(|s| s.to_vec())
        .ok_or_else(|| Error::internal("TIFF row out of bounds"))
}

fn row_planar<T: Copy + Default>(data: &[T], row: u32, w: usize, _h: usize, spp: usize) -> Result<Vec<T>, Error> {
    let total = data.len();
    let plane_len = total / spp; // derive from actual buffer size, not w*h
    let mut out = Vec::with_capacity(w * spp);
    for ch in 0..spp {
        let start = ch * plane_len + row as usize * w;
        let end = (start + w).min(total);
        let avail = end.saturating_sub(start);
        out.extend_from_slice(data.get(start..end).unwrap_or(&[]));
        // pad with zeros if row is partial
        for _ in avail..w {
            out.push(T::default());
        }
    }
    Ok(out)
}
