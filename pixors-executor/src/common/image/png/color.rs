use crate::common::color::primaries::{RgbPrimaries, WhitePoint};
use crate::common::color::space::ColorSpace;
use crate::common::color::transfer::TransferFn;

use ::png;

/// Detect the color space from a PNG's metadata chunks.
/// Priority: cICP > iCCP > sRGB > gAMA+cHRM > gAMA alone > default sRGB.
pub fn detect_color_space(info: &png::Info) -> ColorSpace {
    use crate::common::color::detect;

    // Priority 1: cICP chunk (ITU-T H.273 / ISO 23091-2)
    if let Some(cicp) = info.coding_independent_code_points {
        // (primaries, white_point) per H.273 Table 2
        let primaries: Option<(RgbPrimaries, WhitePoint)> = match cicp.color_primaries {
            1 => Some((RgbPrimaries::Bt709, WhitePoint::D65)),
            9 => Some((RgbPrimaries::Bt2020, WhitePoint::D65)),
            // 10 = XYZ — no RGB primaries, skip
            11 => Some((RgbPrimaries::P3, WhitePoint::P3Dci)), // DCI P3
            12 => Some((RgbPrimaries::P3, WhitePoint::D65)),   // Display P3
            _ => None,
        };
        let transfer = match cicp.transfer_function {
            1 | 6 | 14 | 15 => Some(TransferFn::Rec709Gamma),
            2 | 3 => Some(TransferFn::Gamma22),
            4 => Some(TransferFn::Gamma22), // BT.470M ~2.2
            5 => Some(TransferFn::Gamma26), // BT.470BG ~2.8; Gamma26 is closest
            7 | 11 => Some(TransferFn::SrgbGamma),
            8 => Some(TransferFn::Linear),
            13 => Some(TransferFn::SrgbGamma),
            16 => Some(TransferFn::Pq),
            17 | 18 => Some(TransferFn::Hlg),
            _ => None,
        };
        if let (Some((prim, wp)), Some(tf)) = (primaries, transfer) {
            return ColorSpace::new(prim, wp, tf);
        }
    }

    // Priority 2: iCCP chunk
    if let Some(icc_bytes) = &info.icc_profile {
        let classified = detect::IccClassification::classify_icc_profile(icc_bytes);
        if let Some(cs) = classified.color_space {
            return cs;
        }
        tracing::warn!(
            "Unrecognized ICC profile (desc: {}), assuming sRGB",
            String::from_utf8_lossy(&classified.raw)
                .chars()
                .take(60)
                .collect::<String>()
        );
        return ColorSpace::SRGB;
    }

    // Priority 3: sRGB chunk
    if info.srgb.is_some() {
        return ColorSpace::SRGB;
    }

    // Priority 4: gAMA + cHRM chunks (use shared chromaticity matcher)
    let mut gamma = None;
    if let Some(g) = info.gamma() {
        gamma = Some(g.into_value());
    }
    if let Some(chrm) = info.chromaticities() {
        if let Some((prim, wp)) = detect::match_chromaticities(
            chrm.white.0.into_value(),
            chrm.white.1.into_value(),
            chrm.red.0.into_value(),
            chrm.red.1.into_value(),
            chrm.green.0.into_value(),
            chrm.green.1.into_value(),
            chrm.blue.0.into_value(),
            chrm.blue.1.into_value(),
            0.002,
        ) {
            let transfer = gamma
                .and_then(TransferFn::from_gamma)
                .unwrap_or(TransferFn::SrgbGamma);
            return ColorSpace::new(prim, wp, transfer);
        }
        if let Some(g) = gamma
            && let Some(tf) = TransferFn::from_gamma(g)
        {
            return ColorSpace::with_optional_params(None, None, Some(tf));
        }
    }

    // Priority 5: gAMA alone
    if let Some(g) = gamma
        && let Some(tf) = TransferFn::from_gamma(g)
    {
        return ColorSpace::with_optional_params(None, None, Some(tf));
    }

    // No color info → assume sRGB
    tracing::warn!("No color space metadata in PNG, assuming sRGB");
    ColorSpace::SRGB
}
