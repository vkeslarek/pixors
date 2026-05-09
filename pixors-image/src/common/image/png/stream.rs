use std::fs::File;
use std::io::BufReader;

use ::png;

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::PixelFormat;
use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_engine::data::buffer::Buffer;
use pixors_engine::data::scanline::ScanLine;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;

use crate::common::image::codec::PageStream;
use crate::common::image::*;

pub struct PngPageStream {
    reader: png::Reader<BufReader<File>>,
    pub page_info: PageInfo,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    is_16bit: bool,
    bit_depth: u8,
    color_type: png::ColorType,
    row: u32,
    height: u32,
    width: u32,
    done: bool,
}

impl PngPageStream {
    pub fn new(
        reader: png::Reader<BufReader<File>>,
        page_info: PageInfo,
        pixel_format: PixelFormat,
        color_space: ColorSpace,
        is_16bit: bool,
        bit_depth: u8,
        color_type: png::ColorType,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            reader,
            page_info,
            pixel_format,
            color_space,
            is_16bit,
            bit_depth,
            color_type,
            row: 0,
            height,
            width,
            done: false,
        }
    }
}

impl PageStream for PngPageStream {
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
            self.color_space,
            self.page_info.alpha_policy,
        );
        let w = self.width;

        for _ in 0..count {
            match self
                .reader
                .next_row()
                .map_err(|e| Error::Png(e.to_string()))?
            {
                Some(row_data) => {
                    let mut raw = row_data.data().to_vec();

                    // Scale sub-byte grayscale to full 0..255 range.
                    // png EXPAND widths values to bytes but does NOT scale (e.g. 1-bit → 0/1).
                    if self.bit_depth < 8 && matches!(self.color_type, png::ColorType::Grayscale) {
                        let mul = 255u8 / ((1u8 << self.bit_depth) - 1);
                        if self.pixel_format == PixelFormat::GrayA8 {
                            // tRNS expansion produced [gray, alpha] pairs — scale only gray
                            for chunk in raw.chunks_exact_mut(2) {
                                chunk[0] = chunk[0].saturating_mul(mul);
                            }
                        } else {
                            for b in &mut raw {
                                *b = b.saturating_mul(mul);
                            }
                        }
                    }

                    // 16-bit: PNG stores big-endian, swap to native
                    if self.is_16bit {
                        for chunk in raw.chunks_exact_mut(2) {
                            chunk.swap(0, 1);
                        }
                    }

                    items.push(Item::ScanLine(ScanLine::new(
                        0,
                        self.row,
                        w,
                        meta,
                        Buffer::cpu(raw),
                    )));
                    self.row += 1;
                }
                None => {
                    self.done = true;
                    break;
                }
            }
        }
        if self.row >= self.height {
            self.done = true;
        }
        Ok(items)
    }
}

/// Determine PixelFormat from PNG info. Takes tRNS into account — the EXPAND
/// transformation inserts alpha bytes when tRNS is present, so the format must
/// reflect that.
pub fn png_pixel_format(info: &png::Info, is_16bit: bool) -> PixelFormat {
    let has_trns = info.trns.is_some();
    match info.color_type {
        png::ColorType::Grayscale => {
            if has_trns {
                if is_16bit {
                    PixelFormat::GrayA16
                } else {
                    PixelFormat::GrayA8
                }
            } else if is_16bit {
                PixelFormat::Gray16
            } else {
                PixelFormat::Gray8
            }
        }
        png::ColorType::GrayscaleAlpha => {
            if is_16bit {
                PixelFormat::GrayA16
            } else {
                PixelFormat::GrayA8
            }
        }
        png::ColorType::Rgb => {
            if has_trns {
                if is_16bit {
                    PixelFormat::Rgba16
                } else {
                    PixelFormat::Rgba8
                }
            } else if is_16bit {
                PixelFormat::Rgb16
            } else {
                PixelFormat::Rgb8
            }
        }
        png::ColorType::Rgba => {
            if is_16bit {
                PixelFormat::Rgba16
            } else {
                PixelFormat::Rgba8
            }
        }
        png::ColorType::Indexed => {
            if has_trns {
                PixelFormat::Rgba8
            } else {
                PixelFormat::Rgb8
            }
        }
    }
}
