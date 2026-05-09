use std::fs::File;
use std::io::BufReader;

use tiff;

use crate::image::Orientation;
use pixors_engine::common::color::space::ColorSpace;

// ── Missing from tiff::tags::Tag enum ──────────────────────────────────────
const TAG_PAGE_NAME: u16 = 285;
const TAG_X_POSITION: u16 = 286;
const TAG_Y_POSITION: u16 = 287;
const TAG_YCBCR_SUBSAMPLING: u16 = 530;

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
        .get_tag_ascii_string(tiff::tags::Tag::Unknown(TAG_PAGE_NAME))
        .ok()
        .filter(|s| !s.is_empty())
}

/// Read a RATIONAL tag as f32 (numerator / denominator).
/// Falls back to integer value if the tag is stored as unsigned.
pub fn read_rational_tag(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
    tag: tiff::tags::Tag,
) -> Option<f32> {
    use tiff::decoder::ifd::Value;
    match decoder.find_tag(tag).ok()? {
        Some(Value::Rational(num, den)) if den != 0 => Some(num as f32 / den as f32),
        Some(Value::List(vals)) if !vals.is_empty() => {
            if let Value::Rational(num, den) = vals[0] {
                if den != 0 {
                    Some(num as f32 / den as f32)
                } else {
                    None
                }
            } else {
                None
            }
        }
        Some(Value::Short(v)) => Some(v as f32),
        Some(Value::Unsigned(v)) => Some(v as f32),
        _ => None,
    }
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
        let classified =
            pixors_engine::common::color::detect::IccClassification::classify_icc_profile(icc);
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
            // CIELab (8): model transform outputs linear sRGB coordinates — must use
            // linear transfer so the pipeline does not apply sRGB gamma a second time.
            Some(8) => return ColorSpace::LINEAR_SRGB,
            _ => {}
        }
    }

    tracing::warn!("No color space metadata in TIFF, assuming sRGB");
    ColorSpace::SRGB
}

/// Read ExtraSamples tag (338). Returns the extra sample type if present.
/// 0 = unspecified, 1 = associated (premultiplied), 2 = unassociated (straight).
pub fn read_extra_samples(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> Option<u32> {
    decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::ExtraSamples)
        .ok()
        .flatten()
}

/// Read EXIF blob from TIFF sub-IFD (tag 34665).
pub fn read_exif_blob(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> Option<Vec<u8>> {
    decoder
        .get_tag_u8_vec(tiff::tags::Tag::ExifDirectory)
        .ok()
        .filter(|v| !v.is_empty())
}

/// Read ICC profile bytes (tag 34675).
pub fn read_icc_profile(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> Option<Vec<u8>> {
    decoder
        .get_tag_u8_vec(tiff::tags::Tag::IccProfile)
        .ok()
        .filter(|v| !v.is_empty())
}

/// Read PlanarConfiguration tag (284). Returns true if planar (chunky = false).
pub fn read_planar_config(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> bool {
    decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::PlanarConfiguration)
        .ok()
        .flatten()
        .map(|v| v == 2)
        .unwrap_or(false)
}

/// Read an ASCII string tag.
pub fn read_tag_ascii(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
    tag: tiff::tags::Tag,
) -> Option<String> {
    decoder.get_tag_ascii_string(tag).ok()
}

/// Read ColorMap tag (320) — 3 × 2^bits u16 entries (R section, G section, B section).
/// TIFF stores colormap values as u16 in [0..65535]. Returns None if tag absent or malformed.
pub fn read_color_map(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> Option<Vec<u16>> {
    use tiff::decoder::ifd::Value;
    match decoder.find_tag(tiff::tags::Tag::ColorMap).ok()? {
        Some(Value::List(list)) => {
            let mut out = Vec::with_capacity(list.len());
            for v in list {
                match v {
                    Value::Short(s) => out.push(s),
                    Value::Unsigned(u) => out.push(u as u16),
                    _ => return None,
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

/// Returns true when PhotometricInterpretation == 0 (WhiteIsZero — inverted gray).
pub fn read_white_is_zero(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> bool {
    decoder
        .find_tag_unsigned::<u32>(tiff::tags::Tag::PhotometricInterpretation)
        .ok()
        .flatten()
        .map(|v| v == 0)
        .unwrap_or(false)
}

/// Read YCbCrSubSampling tag (530) — horizontal and vertical factors.
/// Returns (1, 1) when absent (no subsampling).
pub fn read_ycbcr_subsampling(decoder: &mut tiff::decoder::Decoder<BufReader<File>>) -> (u8, u8) {
    use tiff::decoder::ifd::Value;
    let val = decoder
        .find_tag(tiff::tags::Tag::Unknown(TAG_YCBCR_SUBSAMPLING))
        .ok()
        .flatten();
    match val {
        Some(Value::List(list)) if list.len() >= 2 => {
            let h = match &list[0] {
                Value::Short(s) => *s as u8,
                Value::Unsigned(u) => *u as u8,
                _ => 1,
            };
            let v = match &list[1] {
                Value::Short(s) => *s as u8,
                Value::Unsigned(u) => *u as u8,
                _ => 1,
            };
            (h, v)
        }
        Some(Value::Short(s)) => (s as u8, 1),
        Some(Value::Unsigned(u)) => (u as u8, 1),
        _ => (1, 1),
    }
}
