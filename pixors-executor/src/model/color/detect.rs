//! Shared color-space detection helpers.
//!
//! Used by PNG and TIFF readers. Pure functions, no I/O.


// ---------------------------------------------------------------------------
// Chromaticity matching
// ---------------------------------------------------------------------------

use crate::model::color::primaries::{RgbPrimaries, WhitePoint};
use crate::model::color::space::ColorSpace;

/// Match `(wx, wy, rx, ry, gx, gy, bx, by)` chromaticity values against
/// known primaries+whitepoint combos. Returns `(RgbPrimaries, WhitePoint)`
/// if the values match within `tol` (recommended: 0.002 for TIFF, 0.001 for PNG).
pub fn match_chromaticities(
    wx: f32,
    wy: f32,
    rx: f32,
    ry: f32,
    gx: f32,
    gy: f32,
    bx: f32,
    by: f32,
    tol: f32,
) -> Option<(RgbPrimaries, WhitePoint)> {
    let known: &[(
        RgbPrimaries,
        WhitePoint,
        (f32, f32, f32, f32, f32, f32, f32, f32),
    )] = &[
        (
            RgbPrimaries::Bt709,
            WhitePoint::D65,
            (0.3127, 0.3290, 0.640, 0.330, 0.300, 0.600, 0.150, 0.060),
        ),
        (
            RgbPrimaries::Adobe1998,
            WhitePoint::D65,
            (0.3127, 0.3290, 0.640, 0.330, 0.210, 0.710, 0.150, 0.060),
        ),
        (
            RgbPrimaries::P3,
            WhitePoint::D65,
            (0.3127, 0.3290, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060),
        ),
        (
            RgbPrimaries::Bt2020,
            WhitePoint::D65,
            (0.3127, 0.3290, 0.708, 0.292, 0.170, 0.797, 0.131, 0.046),
        ),
        (
            RgbPrimaries::Ap0,
            WhitePoint::D60,
            (0.32168, 0.33767, 0.7347, 0.2653, 0.0, 1.0, 0.0001, -0.077),
        ),
        (
            RgbPrimaries::Ap1,
            WhitePoint::D60,
            (0.32168, 0.33767, 0.713, 0.293, 0.165, 0.830, 0.128, 0.044),
        ),
        (
            RgbPrimaries::ProPhoto,
            WhitePoint::D50,
            (
                0.3457, 0.3585, 0.7347, 0.2653, 0.1596, 0.8404, 0.0366, 0.0001,
            ),
        ),
    ];

    for (prim, wp, (kwx, kwy, krx, kry, kgx, kgy, kbx, kby)) in known {
        if (wx - kwx).abs() < tol
            && (wy - kwy).abs() < tol
            && (rx - krx).abs() < tol
            && (ry - kry).abs() < tol
            && (gx - kgx).abs() < tol
            && (gy - kgy).abs() < tol
            && (bx - kbx).abs() < tol
            && (by - kby).abs() < tol
        {
            return Some((*prim, *wp));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// ICC profile classification
// ---------------------------------------------------------------------------

/// Result of ICC profile classification.
pub struct IccClassification {
    pub color_space: Option<ColorSpace>,
    pub raw: Vec<u8>,
}

impl IccClassification {
    /// Classify an ICC profile by parsing header + desc tag string match.
    /// Returns a known `ColorSpace` only when `profile_class == "mntr"` (display),
    /// `color_space == "RGB "` (four bytes), and the description matches a known entry.
    pub fn classify_icc_profile(bytes: &[u8]) -> Self {
        if bytes.len() < 128 {
            return Self {
                color_space: None,
                raw: bytes.to_vec(),
            };
        }

        let profile_class = &bytes[12..16];
        let color_space_sig = &bytes[16..20];

        if profile_class != b"mntr" || color_space_sig != b"RGB " {
            return Self {
                color_space: None,
                raw: bytes.to_vec(),
            };
        }

        let desc_name = Self::extract_desc_text(bytes);
        let norm = Self::normalise_profile_name(&desc_name);

        let known: &[(&str, ColorSpace)] = &[
            ("srgb iec61966 2 1", ColorSpace::SRGB),
            ("srgb iec61966 2 1 relative colorimetric", ColorSpace::SRGB),
            ("srgb", ColorSpace::SRGB),
            ("adobe rgb 1998", ColorSpace::ADOBE_RGB),
            ("display p3", ColorSpace::DISPLAY_P3),
            ("dci p3 d65", ColorSpace::DCI_P3),
            ("prophoto rgb", ColorSpace::PROPHOTO),
            ("rec2020", ColorSpace::REC2020),
        ];

        for (name, cs) in known {
            if norm.contains(name) {
                return Self {
                    color_space: Some(*cs),
                    raw: bytes.to_vec(),
                };
            }
        }

        Self {
            color_space: None,
            raw: bytes.to_vec(),
        }
    }

    fn normalise_profile_name(s: &str) -> String {
        let lower = s.to_lowercase();
        let mut out = String::with_capacity(lower.len());
        let mut prev_space = false;
        for ch in lower.chars() {
            if ch.is_alphanumeric() {
                out.push(ch);
                prev_space = false;
            } else if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        }
        out.trim().to_string()
    }

    fn extract_desc_text(bytes: &[u8]) -> String {
        if bytes.len() < 132 {
            return String::new();
        }
        let tag_count =
            u32::from_be_bytes([bytes[128], bytes[129], bytes[130], bytes[131]]) as usize;
        let tag_table_start = 132;
        let desc_tag = 0x64657363u32; // 'desc'

        for i in 0..tag_count.min(64) {
            let off = tag_table_start + i * 12;
            if off + 12 > bytes.len() {
                break;
            }
            let tag =
                u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
            if tag == desc_tag {
                let data_off = u32::from_be_bytes([
                    bytes[off + 4],
                    bytes[off + 5],
                    bytes[off + 6],
                    bytes[off + 7],
                ]) as usize;
                let data_len = u32::from_be_bytes([
                    bytes[off + 8],
                    bytes[off + 9],
                    bytes[off + 10],
                    bytes[off + 11],
                ]) as usize;
                if data_off
                    .checked_add(data_len)
                    .is_none_or(|end| end > bytes.len())
                {
                    break;
                }
                let start = data_off + 4;
                let end = (start..start + data_len.saturating_sub(4))
                    .find(|&pos| bytes[pos] == 0)
                    .unwrap_or(start + data_len.saturating_sub(4));
                return String::from_utf8_lossy(&bytes[start..end.min(bytes.len())]).into_owned();
            }
        }
        String::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::model::color::transfer::TransferFn;
    use super::*;

    #[test]
    fn match_srgb_chromaticities() {
        let result = match_chromaticities(
            0.3127, 0.3290, 0.640, 0.330, 0.300, 0.600, 0.150, 0.060, 0.001,
        );
        assert_eq!(result, Some((RgbPrimaries::Bt709, WhitePoint::D65)));
    }

    #[test]
    fn match_p3_chromaticities() {
        let result = match_chromaticities(
            0.3127, 0.3290, 0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.001,
        );
        assert_eq!(result, Some((RgbPrimaries::P3, WhitePoint::D65)));
    }

    #[test]
    fn no_match_out_of_tolerance() {
        assert!(match_chromaticities(0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.001,).is_none());
    }

    #[test]
    fn known_gamma_mapping() {
        assert_eq!(TransferFn::from_gamma(1.0 / 2.2), Some(TransferFn::Gamma22));
        assert_eq!(TransferFn::from_gamma(1.0 / 2.4), Some(TransferFn::Gamma24));
        assert_eq!(TransferFn::from_gamma(0.999), Some(TransferFn::Linear));
        assert_eq!(TransferFn::from_gamma(3.0), None);
    }

    #[test]
    fn icc_name_normalisation() {
        let norm = |s: &str| IccClassification::normalise_profile_name(s);
        assert_eq!(norm("sRGB IEC61966-2.1"), "srgb iec61966 2 1");
        assert_eq!(norm("sRGB_IEC61966_2_1"), "srgb iec61966 2 1");
        assert_eq!(norm("Adobe RGB (1998)"), "adobe rgb 1998");
        assert_eq!(norm("Display P3"), "display p3");
    }

    #[test]
    fn icc_rejects_non_mntr_or_non_rgb() {
        let mut bytes = vec![0u8; 132];
        bytes[12..16].copy_from_slice(b"scnr");
        bytes[16..20].copy_from_slice(b"RGB ");
        let result = IccClassification::classify_icc_profile(&bytes);
        assert!(result.color_space.is_none());
    }

    #[test]
    fn gamma_mapping_edge_cases() {
        assert_eq!(
            TransferFn::from_gamma(1.0 / 2.21),
            Some(TransferFn::Gamma22)
        );
        assert!(TransferFn::from_gamma(0.3).is_none());
    }
}
