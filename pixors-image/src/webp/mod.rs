use std::path::Path;

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::error::Error;
use webp::Decoder as WebpDecoder;

use crate::codec::{ImageDecoder, PageStream};
use crate::image::*;

pub struct WebPDecoder;

impl ImageDecoder for WebPDecoder {
    fn probe(&self, path: &Path) -> Result<bool, Error> {
        Ok(path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("webp"))
            .unwrap_or(false))
    }

    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error> {
        let data = std::fs::read(path).map_err(Error::Io)?;
        let decoder = WebpDecoder::new(&data);
        let img = decoder
            .decode()
            .ok_or_else(|| Error::internal("WebP decode failed"))?;
        let w = img.width();
        let h = img.height();
        let has_alpha = img.is_alpha();
        let fmt = if has_alpha {
            PixelFormat::Rgba8
        } else {
            PixelFormat::Rgb8
        };
        let ap = if has_alpha {
            AlphaPolicy::Straight
        } else {
            AlphaPolicy::OpaqueDrop
        };

        let pages = vec![PageInfo {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("webp")
                .to_string(),
            color_space: ColorSpace::SRGB,
            alpha_policy: ap,
            offset: PixelOffset { x: 0, y: 0 },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            orientation: Orientation::Identity,
            delay_ms: 0,
            dispose: DisposeOp::None,
        }];

        Ok(ImageDescriptor {
            format: format!("{:?}", fmt),
            width: w,
            height: h,
            bit_depth: 8,
            color_space: ColorSpace::SRGB,
            dpi: None,
            metadata: Vec::new(),
            icc_profile: None,
            pages,
        })
    }

    fn open_stream(&self, path: &Path, _page: usize) -> Result<Box<dyn PageStream>, Error> {
        let data = std::fs::read(path).map_err(Error::Io)?;
        let decoder = WebpDecoder::new(&data);
        let img = decoder
            .decode()
            .ok_or_else(|| Error::internal("WebP decode failed"))?;
        let w = img.width();
        let h = img.height();
        let has_alpha = img.is_alpha();
        let fmt = if has_alpha {
            PixelFormat::Rgba8
        } else {
            PixelFormat::Rgb8
        };
        let ap = if has_alpha {
            AlphaPolicy::Straight
        } else {
            AlphaPolicy::OpaqueDrop
        };
        let bpp = if has_alpha { 4 } else { 3 };
        let pixels: Vec<u8> = img.to_vec();

        let page_info = PageInfo {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("webp")
                .to_string(),
            color_space: ColorSpace::SRGB,
            alpha_policy: ap,
            offset: PixelOffset { x: 0, y: 0 },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            orientation: Orientation::Identity,
            delay_ms: 0,
            dispose: DisposeOp::None,
        };

        Ok(Box::new(WebPPageStream {
            pixels,
            page_info,
            pixel_format: fmt,
            bpp,
            width: w,
            height: h,
            row: 0,
        }))
    }
}

struct WebPPageStream {
    pixels: Vec<u8>,
    page_info: PageInfo,
    pixel_format: PixelFormat,
    bpp: usize,
    width: u32,
    height: u32,
    row: u32,
}

impl PageStream for WebPPageStream {
    fn page_info(&self) -> &PageInfo {
        &self.page_info
    }

    fn drain(&mut self, max_items: usize) -> Result<Vec<pixors_engine::graph::item::Item>, Error> {
        use pixors_engine::common::pixel::meta::PixelMeta;
        use pixors_engine::data::buffer::Buffer;
        use pixors_engine::data::scanline::ScanLine;
        use pixors_engine::graph::item::Item;

        let meta = PixelMeta::new(
            self.pixel_format,
            self.page_info.color_space,
            self.page_info.alpha_policy,
        );
        let mut items = Vec::new();
        let count = max_items.min((self.height - self.row) as usize);
        for _ in 0..count {
            let row_bytes = self.width as usize * self.bpp;
            let start = self.row as usize * row_bytes;
            let end = start + row_bytes;
            if end > self.pixels.len() {
                break;
            }
            items.push(Item::ScanLine(ScanLine::new(
                0,
                self.row,
                self.width,
                meta,
                Buffer::cpu(self.pixels[start..end].to_vec()),
            )));
            self.row += 1;
        }
        Ok(items)
    }
}
