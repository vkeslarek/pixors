mod tags;
mod stream;

pub use stream::{tiff_pixel_format, tiff_row_bytes, TiffPageStream};
pub use tags::{
    count_tiff_pages, detect_tiff_color_space, read_orientation, read_page_name,
    read_page_offset,
};

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ::tiff as tiff;

use crate::error::Error;
use crate::common::pixel::AlphaPolicy;

use super::codec::{ImageDecoder, PageStream};
use super::*;

pub struct TiffDecoder;

impl ImageDecoder for TiffDecoder {
    fn probe(&self, path: &Path) -> Result<bool, Error> {
        Ok(path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("tiff") || e.eq_ignore_ascii_case("tif"))
            .unwrap_or(false))
    }

    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = tiff::decoder::Decoder::new(reader)
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = decoder
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder
            .colortype()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let bit_depth = ct.bit_depth();
        let color_space = detect_tiff_color_space(&mut decoder);

        let dpi = {
            let xres = decoder
                .find_tag_unsigned::<u32>(tiff::tags::Tag::XResolution)
                .ok()
                .flatten();
            let yres = decoder
                .find_tag_unsigned::<u32>(tiff::tags::Tag::YResolution)
                .ok()
                .flatten();
            match (xres, yres) {
                (Some(x), Some(y)) => {
                    let unit = decoder
                        .find_tag_unsigned::<u32>(tiff::tags::Tag::ResolutionUnit)
                        .ok()
                        .flatten()
                        .unwrap_or(2);
                    let scale = if unit == 3 { 2.54 } else { 1.0 };
                    Some(Dpi { x: x as f32 * scale, y: y as f32 * scale })
                }
                _ => None,
            }
        };
        let icc_profile = decoder
            .get_tag_u8_vec(tiff::tags::Tag::IccProfile)
            .ok();

        let mut metadata = Vec::new();
        metadata.push(Metadata::ImageWidth(w));
        metadata.push(Metadata::ImageHeight(h));
        metadata.push(Metadata::PhotometricInterpretation(2));

        if let Some(ref dpi_val) = dpi {
            metadata.push(Metadata::Dpi { x: dpi_val.x, y: dpi_val.y });
        }

        if let Some(ref icc) = icc_profile
            && !icc.is_empty()
        {
            metadata.push(Metadata::IccProfile(icc.clone()));
        }

        let first_orientation = read_orientation(&mut decoder);
        metadata.push(Metadata::Orientation(first_orientation as u16));

        let page_count = count_tiff_pages(&mut decoder);
        let mut pages = Vec::with_capacity(page_count);

        for i in 0..page_count {
            decoder
                .seek_to_image(i)
                .map_err(|e| Error::Tiff(e.to_string()))?;
            let (pw, ph) = decoder
                .dimensions()
                .map_err(|e| Error::Tiff(e.to_string()))?;
            let pct = decoder
                .colortype()
                .map_err(|e| Error::Tiff(e.to_string()))?;
            let pcs = detect_tiff_color_space(&mut decoder);
            let name = read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", i + 1));
            let (ox, oy) = read_page_offset(&mut decoder, pw, ph);
            let orientation = read_orientation(&mut decoder);

            let is_alpha =
                matches!(pct, tiff::ColorType::RGBA(..) | tiff::ColorType::GrayA(..));
            pages.push(PageInfo {
                name,
                color_space: pcs,
                alpha_policy: if is_alpha {
                    AlphaPolicy::Straight
                } else {
                    AlphaPolicy::OpaqueDrop
                },
                offset: PixelOffset { x: ox, y: oy },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
            });
        }

        Ok(ImageDescriptor {
            format: "TIFF".to_string(),
            width: w,
            height: h,
            bit_depth,
            color_space,
            dpi,
            metadata,
            icc_profile,
            pages,
        })
    }

    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error> {
        let file = File::open(path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut decoder = tiff::decoder::Decoder::new(reader)
            .map_err(|e| Error::Tiff(e.to_string()))?;
        decoder
            .seek_to_image(page)
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let (w, h) = decoder
            .dimensions()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let ct = decoder
            .colortype()
            .map_err(|e| Error::Tiff(e.to_string()))?;
        let cs = detect_tiff_color_space(&mut decoder);
        let name =
            read_page_name(&mut decoder).unwrap_or_else(|| format!("Page {}", page + 1));
        let (ox, oy) = read_page_offset(&mut decoder, w, h);
        let orientation = read_orientation(&mut decoder);

        let image_data = decoder
            .read_image()
            .map_err(|e| Error::Tiff(e.to_string()))?;

        let pixel_format = tiff_pixel_format(&image_data, ct);
        let is_alpha = matches!(ct, tiff::ColorType::RGBA(..) | tiff::ColorType::GrayA(..));

        Ok(Box::new(TiffPageStream::new(
            PageInfo {
                name,
                color_space: cs,
                alpha_policy: if is_alpha {
                    AlphaPolicy::Straight
                } else {
                    AlphaPolicy::OpaqueDrop
                },
                offset: PixelOffset { x: ox, y: oy },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                orientation,
            },
            image_data,
            ct,
            pixel_format,
            w,
            h,
        )))
    }
}
