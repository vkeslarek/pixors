use std::path::Path;

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::error::Error;
use zune_jpeg::JpegDecoder as ZuneDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace as ZuneColorSpace;

use crate::codec::{ImageDecoder, PageStream};
use crate::image::*;

pub struct JpegDecoder;

impl ImageDecoder for JpegDecoder {
    fn probe(&self, path: &Path) -> Result<bool, Error> {
        Ok(path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
            .unwrap_or(false))
    }

    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error> {
        let data = std::fs::read(path).map_err(Error::Io)?;
        let mut decoder = ZuneDecoder::new(&data);
        decoder
            .decode_headers()
            .map_err(|e| Error::internal(e.to_string()))?;
        let info = decoder
            .info()
            .ok_or_else(|| Error::internal("no JPEG info"))?;

        let (fmt, color_space) = match decoder.get_input_colorspace() {
            Some(ZuneColorSpace::Luma) => (PixelFormat::Gray8, ColorSpace::SRGB),
            Some(ZuneColorSpace::CMYK) => (PixelFormat::Cmyk8, ColorSpace::SRGB),
            _ => (PixelFormat::Rgb8, ColorSpace::SRGB),
        };

        let pages = vec![PageInfo {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("jpeg")
                .to_string(),
            color_space,
            alpha_policy: AlphaPolicy::OpaqueDrop,
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
            width: info.width as u32,
            height: info.height as u32,
            bit_depth: 8,
            color_space,
            dpi: None,
            metadata: Vec::new(),
            icc_profile: None,
            pages,
        })
    }

    fn open_stream(&self, path: &Path, _page: usize) -> Result<Box<dyn PageStream>, Error> {
        let data = std::fs::read(path).map_err(Error::Io)?;
        let mut decoder = ZuneDecoder::new(&data);
        decoder
            .decode_headers()
            .map_err(|e| Error::internal(e.to_string()))?;
        let info = decoder
            .info()
            .ok_or_else(|| Error::internal("no JPEG info"))?;

        let bpp = match decoder.get_input_colorspace() {
            Some(ZuneColorSpace::Luma) => 1,
            Some(ZuneColorSpace::CMYK) => 4,
            _ => 3,
        };
        let (fmt, cs) = match decoder.get_input_colorspace() {
            Some(ZuneColorSpace::Luma) => (PixelFormat::Gray8, ColorSpace::SRGB),
            Some(ZuneColorSpace::CMYK) => (PixelFormat::Cmyk8, ColorSpace::SRGB),
            _ => (PixelFormat::Rgb8, ColorSpace::SRGB),
        };

        let w = info.width as u32;
        let h = info.height as u32;
        let pixels = decoder
            .decode()
            .map_err(|e| Error::internal(e.to_string()))?;

        let page_info = PageInfo {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("jpeg")
                .to_string(),
            color_space: cs,
            alpha_policy: AlphaPolicy::OpaqueDrop,
            offset: PixelOffset { x: 0, y: 0 },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            orientation: Orientation::Identity,
            delay_ms: 0,
            dispose: DisposeOp::None,
        };

        Ok(Box::new(JpegPageStream {
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

struct JpegPageStream {
    pixels: Vec<u8>,
    page_info: PageInfo,
    pixel_format: PixelFormat,
    bpp: usize,
    width: u32,
    height: u32,
    row: u32,
}

impl PageStream for JpegPageStream {
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
