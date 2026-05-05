//! RGB primaries and white points.
//!
//! Based on kolor's `RgbPrimaries` and `WhitePoint`.

use crate::model::color::chromaticity::Chromaticity;

/// A set of primary colors that define an RGB color space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum RgbPrimaries {
    /// No primaries (placeholder).
    None,
    /// BT.709 (also used by sRGB).
    Bt709,
    /// BT.2020 (Rec.2020).
    Bt2020,
    /// ACES2065-1 (AP0).
    Ap0,
    /// ACEScg (AP1).
    Ap1,
    /// P3 primaries (used by DCI-P3 and variations).
    P3,
    /// Adobe RGB (1998).
    Adobe1998,
    /// Adobe Wide Gamut RGB.
    AdobeWide,
    /// Apple RGB.
    Apple,
    /// ProPhoto RGB (ROMM).
    ProPhoto,
    /// CIE RGB.
    CieRgb,
    /// Identity matrix (XYZ).
    CieXyz,
    /// Custom primaries defined by chromaticities.
    Custom {
        red: Chromaticity,
        green: Chromaticity,
        blue: Chromaticity,
    },
}

impl RgbPrimaries {
    /// Returns the chromaticities (x, y) of the red, green, and blue primaries.
    pub fn chromaticities(&self) -> [Chromaticity; 3] {
        match self {
            Self::None => [
                Chromaticity::new(0.0, 0.0),
                Chromaticity::new(0.0, 0.0),
                Chromaticity::new(0.0, 0.0),
            ],
            Self::Bt709 => [
                Chromaticity::new(0.64, 0.33),
                Chromaticity::new(0.30, 0.60),
                Chromaticity::new(0.15, 0.06),
            ],
            Self::Bt2020 => [
                Chromaticity::new(0.708, 0.292),
                Chromaticity::new(0.170, 0.797),
                Chromaticity::new(0.131, 0.046),
            ],
            Self::Ap0 => [
                Chromaticity::new(0.7347, 0.2653),
                Chromaticity::new(0.0000, 1.0000),
                Chromaticity::new(0.0001, -0.0770),
            ],
            Self::Ap1 => [
                Chromaticity::new(0.713, 0.293),
                Chromaticity::new(0.165, 0.830),
                Chromaticity::new(0.128, 0.044),
            ],
            Self::P3 => [
                Chromaticity::new(0.680, 0.320),
                Chromaticity::new(0.265, 0.690),
                Chromaticity::new(0.150, 0.060),
            ],
            Self::Adobe1998 => [
                Chromaticity::new(0.64, 0.33),
                Chromaticity::new(0.21, 0.71),
                Chromaticity::new(0.15, 0.06),
            ],
            Self::AdobeWide => [
                Chromaticity::new(0.735, 0.265),
                Chromaticity::new(0.115, 0.826),
                Chromaticity::new(0.157, 0.018),
            ],
            Self::Apple => [
                Chromaticity::new(0.625, 0.34),
                Chromaticity::new(0.28, 0.595),
                Chromaticity::new(0.155, 0.07),
            ],
            Self::ProPhoto => [
                Chromaticity::new(0.734699, 0.265301),
                Chromaticity::new(0.159597, 0.840403),
                Chromaticity::new(0.036598, 0.000105),
            ],
            Self::CieRgb => [
                Chromaticity::new(0.7350, 0.2650),
                Chromaticity::new(0.2740, 0.7170),
                Chromaticity::new(0.1670, 0.0090),
            ],
            Self::CieXyz => [
                Chromaticity::new(1.0, 0.0),
                Chromaticity::new(0.0, 1.0),
                Chromaticity::new(0.0, 0.0),
            ],
            Self::Custom { red, green, blue } => [*red, *green, *blue],
        }
    }
}

/// Defines the white point ("achromatic point") of a color space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum WhitePoint {
    /// No white point (placeholder).
    None,
    /// Incandescent / tungsten.
    A,
    /// Old direct sunlight at noon.
    B,
    /// Old daylight.
    C,
    /// Equal energy.
    E,
    /// ICC profile PCS.
    D50,
    /// Mid‑morning daylight.
    D55,
    /// Daylight (used by ACES).
    D60,
    /// Daylight (used by sRGB, Rec.709, Adobe RGB, Display‑P3).
    D65,
    /// North sky daylight.
    D75,
    /// P3‑DCI theater (greenish, ≈6300K).
    P3Dci,
    /// Cool fluorescent.
    F2,
    /// Daylight fluorescent (D65 simulator).
    F7,
    /// Ultralume 40 (Philips TL84).
    F11,
    /// Custom white point defined by chromaticity.
    Custom(Chromaticity),
}

impl WhitePoint {
    /// Returns the chromaticity (x, y) of this white point.
    pub fn xy(&self) -> Chromaticity {
        match self {
            Self::None => Chromaticity::new(0.0, 0.0),
            // Values from ASTM E308-01 (via kolor and brucelindbloom.com)
            Self::A => Chromaticity::new(0.44757, 0.40745),
            Self::B => Chromaticity::new(0.34842, 0.35161),
            Self::C => Chromaticity::new(0.31006, 0.31616),
            Self::E => Chromaticity::new(1.0 / 3.0, 1.0 / 3.0),
            Self::D50 => Chromaticity::new(0.3457, 0.3585),
            Self::D55 => Chromaticity::new(0.3324, 0.3474),
            Self::D60 => Chromaticity::new(0.32168, 0.33767),
            Self::D65 => Chromaticity::new(0.3127, 0.3290),
            Self::D75 => Chromaticity::new(0.2990, 0.3149),
            Self::P3Dci => Chromaticity::new(0.314, 0.351),
            Self::F2 => Chromaticity::new(0.37208, 0.37529),
            Self::F7 => Chromaticity::new(0.31285, 0.32918),
            Self::F11 => Chromaticity::new(0.38052, 0.37713),
            Self::Custom(chroma) => *chroma,
        }
    }

    /// Returns the XYZ tristimulus values of this white point (Y = 1).
    pub fn xyz(&self) -> [f32; 3] {
        let xy = self.xy();
        if xy.y.abs() <= 1e-12 {
            // Avoid division by zero for WhitePoint::None
            return [0.0, 0.0, 0.0];
        }
        let x = xy.x / xy.y;
        let y = 1.0;
        let z = (1.0 - xy.x - xy.y) / xy.y;
        [x, y, z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn white_point_xyz() {
        let d65 = WhitePoint::D65;
        let xy = d65.xy();
        assert_approx_eq!(xy.x, 0.3127, 1e-4);
        assert_approx_eq!(xy.y, 0.3290, 1e-4);

        let xyz = d65.xyz();
        // Check that Y = 1
        assert_approx_eq!(xyz[1], 1.0, 1e-6);
        // Known approximate values for D65 XYZ
        assert_approx_eq!(xyz[0], 0.95047, 3e-4);
        assert_approx_eq!(xyz[2], 1.08883, 3e-4);
    }

    #[test]
    fn primaries_chromaticities() {
        let prim = RgbPrimaries::Bt709;
        let chroma = prim.chromaticities();
        assert_approx_eq!(chroma[0].x, 0.64, 1e-6);
        assert_approx_eq!(chroma[0].y, 0.33, 1e-6);
        assert_approx_eq!(chroma[1].x, 0.30, 1e-6);
        assert_approx_eq!(chroma[1].y, 0.60, 1e-6);
        assert_approx_eq!(chroma[2].x, 0.15, 1e-6);
        assert_approx_eq!(chroma[2].y, 0.06, 1e-6);
    }
}
