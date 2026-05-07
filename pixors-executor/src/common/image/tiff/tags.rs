use std::fs::File;
use std::io::BufReader;

use ::tiff as tiff;

use crate::common::color::space::ColorSpace;
use super::super::Orientation;

/// Count pages in a TIFF by iterating IFDs.
pub fn count_tiff_pages(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> usize {
    let mut count = 1;
    while decoder.more_images() {
        if decoder.next_image().is_ok() {
            count += 1;
        } else {
            break;
        }
    }
    count
}

pub fn read_page_name(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> Option<String> {
    decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Unknown(285))
        .ok()
        .flatten()
        .map(|_| String::from("(page name tag)"))
}

/// Read page offset from XPosition/YPosition tags (286/287).
pub fn read_page_offset(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
    _w: u32,
    _h: u32,
) -> (i32, i32) {
    let x = decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Unknown(286))
        .ok()
        .flatten();
    let y = decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Unknown(287))
        .ok()
        .flatten();
    match (x, y) {
        (Some(xv), Some(yv)) => (xv as i32, yv as i32),
        _ => (0, 0),
    }
}

/// Read Orientation tag (274) — 1..8 EXIF-style.
pub fn read_orientation(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> Orientation {
    let raw = decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Orientation)
        .ok()
        .flatten();
    match raw {
        Some(2) => Orientation::FlipH,
        Some(3) => Orientation::Rotate180,
        Some(4) => Orientation::FlipV,
        Some(5) => Orientation::Transpose,
        Some(6) => Orientation::Rotate90,
        Some(7) => Orientation::Transverse,
        Some(8) => Orientation::Rotate270,
        _ => Orientation::Identity,
    }
}

pub fn detect_tiff_color_space(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
) -> ColorSpace {
    if let Ok(photometric) =
        decoder.find_tag_unsigned::<u32>(tiff::tags::Tag::PhotometricInterpretation)
    {
        match photometric {
            Some(2) => return ColorSpace::SRGB, // RGB — assume sRGB as baseline
            Some(1) => return ColorSpace::SRGB, // BlackIsZero grayscale → sRGB transfer
            _ => {}
        }
    }

    tracing::warn!("No color space metadata in TIFF, assuming sRGB");
    ColorSpace::SRGB
}
