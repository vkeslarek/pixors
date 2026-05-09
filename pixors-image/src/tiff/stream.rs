use pixors_engine::common::pixel::PixelFormat;
use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::data::scanline::ScanLine;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;

use tiff;

use crate::codec::PageStream;
use crate::image::*;

pub struct TiffPageStream {
    page_info: PageInfo,
    image_data: tiff::decoder::DecodingResult,
    color_type: tiff::ColorType,
    pixel_format: PixelFormat,
    width: u32,
    height: u32,
    planar: bool,
    white_is_zero: bool,
    palette: Option<Vec<u16>>,
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
        white_is_zero: bool,
        palette: Option<Vec<u16>>,
    ) -> Self {
        Self {
            page_info,
            image_data,
            color_type,
            pixel_format,
            width,
            height,
            planar,
            white_is_zero,
            palette,
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
            let mut raw = tiff_row_bytes(
                &self.image_data,
                self.row,
                self.width,
                self.height,
                self.color_type,
                self.planar,
            )?;

            // Palette expansion: each byte is a colormap index → 3 bytes RGB
            if let Some(ref colormap) = self.palette {
                let n = colormap.len() / 3;
                let mut expanded = Vec::with_capacity(raw.len() * 3);
                for &idx in &raw {
                    let i = (idx as usize).min(n.saturating_sub(1));
                    expanded.push((colormap[i] >> 8) as u8);
                    expanded.push((colormap[n + i] >> 8) as u8);
                    expanded.push((colormap[2 * n + i] >> 8) as u8);
                }
                raw = expanded;
            }

            // WhiteIsZero inversion: high value = dark (inverted gray)
            if self.white_is_zero {
                match self.pixel_format {
                    PixelFormat::Gray8 => raw.iter_mut().for_each(|b| *b = 255 - *b),
                    PixelFormat::Gray16 => {
                        for chunk in raw.chunks_exact_mut(2) {
                            let v = u16::from_ne_bytes([chunk[0], chunk[1]]);
                            let bytes = (65535u16 - v).to_ne_bytes();
                            chunk[0] = bytes[0];
                            chunk[1] = bytes[1];
                        }
                    }
                    PixelFormat::GrayF32 => {
                        for chunk in raw.chunks_exact_mut(4) {
                            let v = f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            let bytes = (1.0f32 - v).to_ne_bytes();
                            chunk.copy_from_slice(&bytes);
                        }
                    }
                    _ => {}
                }
            }

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

/// Map (DecodingResult, ColorType, photometric) → PixelFormat.
/// photometric=8 → CIE Lab (Lab8/Lab16).
pub fn tiff_pixel_format(
    result: &tiff::decoder::DecodingResult,
    ct: tiff::ColorType,
    photometric: Option<u32>,
) -> PixelFormat {
    let is_lab = matches!(photometric, Some(8));
    match result {
        tiff::decoder::DecodingResult::U8(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::Gray8,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayA8,
            tiff::ColorType::RGB(_) => {
                if is_lab {
                    PixelFormat::Lab8
                } else {
                    PixelFormat::Rgb8
                }
            }
            tiff::ColorType::RGBA(_) => PixelFormat::Rgba8,
            tiff::ColorType::CMYK(_) => PixelFormat::Cmyk8,
            tiff::ColorType::CMYKA(_) => PixelFormat::CmykA8,
            tiff::ColorType::YCbCr(_) => PixelFormat::YCbCr8,
            tiff::ColorType::Palette(_) => PixelFormat::Rgb8,
            _ => {
                tracing::warn!("Unsupported U8 TIFF color: {:?}, falling back to Rgba8", ct);
                PixelFormat::Rgba8
            }
        },
        tiff::decoder::DecodingResult::U16(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::Gray16,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayA16,
            tiff::ColorType::RGB(_) => {
                if is_lab {
                    PixelFormat::Lab16
                } else {
                    PixelFormat::Rgb16
                }
            }
            tiff::ColorType::RGBA(_) => PixelFormat::Rgba16,
            tiff::ColorType::CMYK(_) => PixelFormat::Cmyk16,
            tiff::ColorType::CMYKA(_) => PixelFormat::CmykA16,
            _ => {
                tracing::warn!(
                    "Unsupported U16 TIFF color: {:?}, falling back to Rgba16",
                    ct
                );
                PixelFormat::Rgba16
            }
        },
        tiff::decoder::DecodingResult::F32(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF32,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayAF32,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF32,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF32,
            tiff::ColorType::CMYK(_) => PixelFormat::CmykF32,
            tiff::ColorType::CMYKA(_) => PixelFormat::CmykAF32,
            tiff::ColorType::YCbCr(_) => PixelFormat::YCbCrF32,
            _ => {
                tracing::warn!(
                    "Unsupported F32 TIFF color: {:?}, falling back to RgbaF32",
                    ct
                );
                PixelFormat::RgbaF32
            }
        },
        tiff::decoder::DecodingResult::U32(_)
        | tiff::decoder::DecodingResult::U64(_)
        | tiff::decoder::DecodingResult::F64(_)
        | tiff::decoder::DecodingResult::I8(_)
        | tiff::decoder::DecodingResult::I16(_)
        | tiff::decoder::DecodingResult::I32(_)
        | tiff::decoder::DecodingResult::I64(_) => match ct {
            tiff::ColorType::Gray(_) | tiff::ColorType::GrayA(_) => PixelFormat::GrayF32,
            tiff::ColorType::RGB(_) | tiff::ColorType::YCbCr(_) => PixelFormat::RgbF32,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF32,
            _ => {
                tracing::warn!(
                    "Unsupported wide-int/signed TIFF color: {:?}, falling back to RgbaF32",
                    ct
                );
                PixelFormat::RgbaF32
            }
        },
        tiff::decoder::DecodingResult::F16(_) => match ct {
            tiff::ColorType::Gray(_) => PixelFormat::GrayF16,
            tiff::ColorType::GrayA(_) => PixelFormat::GrayAF16,
            tiff::ColorType::RGB(_) => PixelFormat::RgbF16,
            tiff::ColorType::RGBA(_) => PixelFormat::RgbaF16,
            tiff::ColorType::CMYK(_) => PixelFormat::CmykF16,
            tiff::ColorType::CMYKA(_) => PixelFormat::CmykAF16,
            tiff::ColorType::YCbCr(_) => PixelFormat::YCbCrF16,
            _ => {
                tracing::warn!(
                    "Unsupported F16 TIFF color: {:?}, falling back to RgbaF16",
                    ct
                );
                PixelFormat::RgbaF16
            }
        },
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
        tiff::decoder::DecodingResult::U16(data) => {
            Ok(row_bytes_u16(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::U32(data) => {
            Ok(row_bytes_u32(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| ((*v as f64 / u32::MAX as f64) as f32).to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::U64(data) => {
            Ok(row_bytes_u64(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| ((*v as f64 / u64::MAX as f64) as f32).to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::F32(data) => {
            Ok(row_bytes_f32(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::F64(data) => {
            Ok(row_bytes_f64(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| (*v as f32).to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::F16(data) => {
            Ok(row_bytes_f16(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| v.to_bits().to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::I8(data) => Ok(row_bytes_i8(data, row, w, h, spp, planar)?
            .iter()
            .flat_map(|v| ((*v as f32 + 128.0) / 255.0).to_ne_bytes())
            .collect()),
        tiff::decoder::DecodingResult::I16(data) => {
            Ok(row_bytes_i16(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| ((*v as f32 + 32768.0) / 65535.0).to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::I32(data) => {
            Ok(row_bytes_i32(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| ((*v as f64 / i32::MAX as f64 * 0.5 + 0.5) as f32).to_ne_bytes())
                .collect())
        }
        tiff::decoder::DecodingResult::I64(data) => {
            Ok(row_bytes_i64(data, row, w, h, spp, planar)?
                .iter()
                .flat_map(|v| ((*v as f64 / i64::MAX as f64 * 0.5 + 0.5) as f32).to_ne_bytes())
                .collect())
        }
    }
}

fn row_bytes_u8(
    data: &[u8],
    row: u32,
    w: usize,
    _h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(w * spp);
    if planar {
        let plane_len = data.len() / spp;
        for ch in 0..spp {
            let start = ch * plane_len + row as usize * w;
            let end = (start + w).min(data.len());
            out.extend_from_slice(data.get(start..end).unwrap_or(&[]));
            let avail = end.saturating_sub(start);
            out.extend(std::iter::repeat_n(0u8, w.saturating_sub(avail)));
        }
    } else {
        let start = row as usize * w * spp;
        out.extend_from_slice(
            data.get(start..start + w * spp)
                .ok_or_else(|| Error::internal("TIFF row out of bounds"))?,
        );
    }
    Ok(out)
}

fn row_bytes_u16(
    data: &[u16],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<u16>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_u32(
    data: &[u32],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<u32>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_u64(
    data: &[u64],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<u64>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_f32(
    data: &[f32],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<f32>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_f64(
    data: &[f64],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<f64>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_f16(
    data: &[half::f16],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<half::f16>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_i8(
    data: &[i8],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<i8>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_i16(
    data: &[i16],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<i16>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_i32(
    data: &[i32],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<i32>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}
fn row_bytes_i64(
    data: &[i64],
    row: u32,
    w: usize,
    h: usize,
    spp: usize,
    planar: bool,
) -> Result<Vec<i64>, Error> {
    if planar {
        row_planar(data, row, w, h, spp)
    } else {
        row_interleaved(data, row, w, spp)
    }
}

fn row_interleaved<T: Copy>(data: &[T], row: u32, w: usize, spp: usize) -> Result<Vec<T>, Error> {
    let start = row as usize * w * spp;
    data.get(start..start + w * spp)
        .map(|s| s.to_vec())
        .ok_or_else(|| Error::internal("TIFF row out of bounds"))
}

fn row_planar<T: Copy + Default>(
    data: &[T],
    row: u32,
    w: usize,
    _h: usize,
    spp: usize,
) -> Result<Vec<T>, Error> {
    let total = data.len();
    let plane_len = total / spp;
    let mut out = Vec::with_capacity(w * spp);
    for ch in 0..spp {
        let start = ch * plane_len + row as usize * w;
        let end = (start + w).min(total);
        let avail = end.saturating_sub(start);
        out.extend_from_slice(data.get(start..end).unwrap_or(&[]));
        for _ in avail..w {
            out.push(T::default());
        }
    }
    Ok(out)
}
