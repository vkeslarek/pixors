use std::fs::File;
use std::io::BufReader;

use ::png as png;

use crate::data::buffer::Buffer;
use crate::data::scanline::ScanLine;
use crate::error::Error;
use crate::graph::item::Item;
use crate::common::color::space::ColorSpace;
use crate::common::pixel::PixelFormat;
use crate::common::pixel::meta::PixelMeta;

use super::super::codec::PageStream;
use super::super::*;

pub struct PngPageStream {
    reader: png::Reader<BufReader<File>>,
    pub page_info: PageInfo,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    is_16bit: bool,
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
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            reader,
            page_info,
            pixel_format,
            color_space,
            is_16bit,
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
        let meta = PixelMeta::new(self.pixel_format, self.color_space, self.page_info.alpha_policy);
        let w = self.width;

        for _ in 0..count {
            match self
                .reader
                .next_row()
                .map_err(|e| Error::Png(e.to_string()))?
            {
                Some(row_data) => {
                    let mut raw = row_data.data().to_vec();
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

pub fn png_pixel_format(info: &png::Info, is_16bit: bool) -> PixelFormat {
    match (info.color_type, is_16bit) {
        (png::ColorType::Grayscale, false) => PixelFormat::Gray8,
        (png::ColorType::Grayscale, true) => PixelFormat::Gray16,
        (png::ColorType::GrayscaleAlpha, false) => PixelFormat::GrayA8,
        (png::ColorType::GrayscaleAlpha, true) => PixelFormat::GrayA16,
        (png::ColorType::Rgb, false) => PixelFormat::Rgb8,
        (png::ColorType::Rgb, true) => PixelFormat::Rgb16,
        (png::ColorType::Rgba, false) => PixelFormat::Rgba8,
        (png::ColorType::Rgba, true) => PixelFormat::Rgba16,
        (png::ColorType::Indexed, _) => {
            if info.trns.is_some() {
                PixelFormat::Rgba8
            } else {
                PixelFormat::Rgb8
            }
        }
    }
}
