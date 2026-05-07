use std::fs::File;
use std::io::BufReader;

use ::tiff as tiff;

use crate::common::color::space::ColorSpace;
use super::super::Orientation;

// ── Missing from tiff::tags::Tag enum ──────────────────────────────────────
const TAG_PAGE_NAME: u16 = 285;
const TAG_X_POSITION: u16 = 286;
const TAG_Y_POSITION: u16 = 287;

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
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Unknown(TAG_PAGE_NAME))
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
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Unknown(TAG_X_POSITION))
        .ok()
        .flatten();
    let y = decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::Unknown(TAG_Y_POSITION))
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
    icc_profile: Option<&[u8]>,
) -> ColorSpace {
    // Priority 1: ICC profile
    if let Some(icc) = icc_profile
        && !icc.is_empty()
    {
        let classified = crate::common::color::detect::IccClassification::classify_icc_profile(icc);
        if let Some(cs) = classified.color_space {
            return cs;
        }
    }

    // Priority 2: PhotometricInterpretation (tag 262)
    if let Ok(photometric) =
        decoder.find_tag_unsigned::<u32>(tiff::tags::Tag::PhotometricInterpretation)
    {
        match photometric {
            Some(2) => return ColorSpace::SRGB,
            Some(1) => return ColorSpace::SRGB,
            _ => {}
        }
    }

    tracing::warn!("No color space metadata in TIFF, assuming sRGB");
    ColorSpace::SRGB
}

/// Read ExtraSamples tag (338). Returns the extra sample type if present.
/// 0 = unspecified, 1 = associated (premultiplied), 2 = unassociated (straight).
pub fn read_extra_samples(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
) -> Option<u32> {
    decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::ExtraSamples)
        .ok()
        .flatten()
}

/// Read EXIF blob from TIFF sub-IFD (tag 34665).
pub fn read_exif_blob(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
) -> Option<Vec<u8>> {
    decoder
        .get_tag_u8_vec(tiff::tags::Tag::ExifDirectory)
        .ok()
        .filter(|v| !v.is_empty())
}

/// Read ICC profile bytes (tag 34675).
pub fn read_icc_profile(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
) -> Option<Vec<u8>> {
    decoder
        .get_tag_u8_vec(tiff::tags::Tag::IccProfile)
        .ok()
        .filter(|v| !v.is_empty())
}
