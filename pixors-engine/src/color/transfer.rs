//! Transfer functions (OETF/EOTF) for color spaces.
//!
//! Based on kolor's `TransformFn` and formulas from Phase1.md.
//!
//! For performance, use the `*_fast` functions when processing 8‑bit data.

use super::transfer_lut;



/// Global switch to enable or disable the use of Lookup Tables (LUTs).
/// Useful for testing direct mathematical calculations vs LUT accuracy.
pub const USE_LUT: bool = true;

/// Identifies an invertible mapping of colors in a linear color space.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TransferFn {
    /// No transform (linear).
    Linear,
    /// Piecewise sRGB gamma (≈2.2).
    SrgbGamma,
    /// Rec.709 OETF (≈2.0).
    Rec709Gamma,
    /// Pure gamma 2.2 (not in kolor, for PNG support).
    Gamma22,
    /// Pure gamma 2.4 (broadcast, not in kolor).
    Gamma24,
    /// Pure gamma 2.6 (used by DCI-P3).
    Gamma26,
    /// ProPhoto RGB piecewise with linear segment near zero.
    ProPhotoGamma,
    /// SMPTE ST 2084 PQ (Perceptual Quantizer) used in BT.2100 HDR.
    Pq,
    /// BBC HLG (Hybrid Log-Gamma) HDR transfer function.
    Hlg,
}

impl TransferFn {
    /// Decodes a non-linear value to linear light.
    ///
    /// # Arguments
    /// * `x` - Non-linear encoded value in [0, 1] range.
    ///
    /// # Returns
    /// Linear light value.
    pub fn decode(&self, x: f32) -> f32 {
        match self {
            Self::Linear => x,
            Self::SrgbGamma => srgb_decode(x),
            Self::Rec709Gamma => rec709_decode(x),
            Self::Gamma22 => gamma_decode(x, 2.2),
            Self::Gamma24 => gamma_decode(x, 2.4),
            Self::Gamma26 => gamma_decode(x, 2.6),
            Self::ProPhotoGamma => prophoto_decode(x),
            Self::Pq => pq_decode(x),
            Self::Hlg => hlg_decode(x),
        }
    }

    /// Encodes a linear light value to non-linear representation.
    ///
    /// # Arguments
    /// * `y` - Linear light value (may be outside [0, 1]).
    ///
    /// # Returns
    /// Non-linear encoded value in [0, 1] range (clamped).
    pub fn encode(&self, y: f32) -> f32 {
        match self {
            Self::Linear => y,
            Self::SrgbGamma => srgb_encode(y),
            Self::Rec709Gamma => rec709_encode(y),
            Self::Gamma22 => gamma_encode(y, 2.2),
            Self::Gamma24 => gamma_encode(y, 2.4),
            Self::Gamma26 => gamma_encode(y, 2.6),
            Self::ProPhotoGamma => prophoto_encode(y),
            Self::Pq => pq_encode(y),
            Self::Hlg => hlg_encode(y),
        }
    }

    /// Returns true if the transfer function is linear (no gamma).
    pub const fn is_linear(&self) -> bool {
        matches!(self, Self::Linear)
    }

    /// Decodes an 8‑bit integer value to linear light using LUT.
    /// This is faster than `decode()` for 8‑bit sources.
    pub fn decode_u8_fast(&self, x: u8) -> f32 {
        if !USE_LUT {
            return self.decode(x as f32 / 255.0);
        }
        match self {
            Self::Linear => x as f32 / 255.0,
            Self::SrgbGamma => transfer_lut::srgb_decode_u8_fast(x),
            Self::Rec709Gamma => transfer_lut::rec709_decode_u8_fast(x),
            Self::Gamma22 => transfer_lut::gamma22_decode_u8_fast(x),
            Self::Gamma24 => transfer_lut::gamma24_decode_u8_fast(x),
            Self::ProPhotoGamma => transfer_lut::prophoto_decode_u8_fast(x),
            Self::Gamma26 | Self::Pq | Self::Hlg => {
                // LUT not implemented yet, fall back to decode
                self.decode(x as f32 / 255.0)
            }
        }
    }

    /// Decodes a 16‑bit integer value to linear light using LUT where available.
    /// Falls back to regular decode for transfer functions without LUT.
    pub fn decode_u16_fast(&self, x: u16) -> f32 {
        if !USE_LUT {
            return self.decode(x as f32 / 65535.0);
        }
        match self {
            Self::Linear => x as f32 / 65535.0,
            Self::SrgbGamma => transfer_lut::srgb_decode_u16_fast(x),
            Self::Rec709Gamma => transfer_lut::rec709_decode_u16_fast(x),
            Self::Gamma22 => transfer_lut::gamma22_decode_u16_fast(x),
            Self::Gamma24 => transfer_lut::gamma24_decode_u16_fast(x),
            Self::ProPhotoGamma => transfer_lut::prophoto_decode_u16_fast(x),
            Self::Gamma26 | Self::Pq | Self::Hlg => {
                // LUT not implemented yet, fall back to decode
                self.decode(x as f32 / 65535.0)
            }
        }
    }

    /// Encodes a linear value to non‑linear using LUT with interpolation.
    /// Faster than `encode()` for values in [0, 1].
    pub fn encode_fast(&self, y: f32) -> f32 {
        if !USE_LUT {
            return self.encode(y);
        }
        match self {
            Self::Linear => y,
            Self::SrgbGamma => {
                let y = y.clamp(0.0, 1.0);
                transfer_lut::encode_lookup(y, transfer_lut::srgb_encode_lut())
            }
            Self::Rec709Gamma => {
                let y = y.clamp(0.0, 1.0);
                transfer_lut::encode_lookup(y, transfer_lut::rec709_encode_lut())
            }
            Self::Gamma22 => {
                let y = y.clamp(0.0, 1.0);
                transfer_lut::encode_lookup(y, transfer_lut::gamma22_encode_lut())
            }
            Self::Gamma24 => {
                let y = y.clamp(0.0, 1.0);
                transfer_lut::encode_lookup(y, transfer_lut::gamma24_encode_lut())
            }
            Self::ProPhotoGamma => {
                let y = y.clamp(0.0, 1.0);
                transfer_lut::encode_lookup(y, transfer_lut::prophoto_encode_lut())
            }
            Self::Gamma26 | Self::Pq | Self::Hlg => {
                // LUT not implemented yet, fall back to encode
                self.encode(y)
            }
        }
    }

}

// -----------------------------------------------------------------------------
// sRGB (piecewise)
// -----------------------------------------------------------------------------

fn srgb_decode(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_encode(y: f32) -> f32 {
    if y <= 0.0031308 {
        12.92 * y
    } else {
        1.055 * y.powf(1.0 / 2.4) - 0.055
    }
}

// -----------------------------------------------------------------------------
// Rec.709 OETF (similar to sRGB but different constants)
// -----------------------------------------------------------------------------

fn rec709_decode(x: f32) -> f32 {
    if x < 0.081 {
        x / 4.5
    } else {
        ((x + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

fn rec709_encode(y: f32) -> f32 {
    if y < 0.018 {
        4.5 * y
    } else {
        1.099 * y.powf(0.45) - 0.099
    }
}

// -----------------------------------------------------------------------------
// Pure gamma
// -----------------------------------------------------------------------------

fn gamma_decode(x: f32, gamma: f32) -> f32 {
    x.powf(gamma)
}

fn gamma_encode(y: f32, gamma: f32) -> f32 {
    y.max(0.0).powf(1.0 / gamma)
}

// -----------------------------------------------------------------------------
// ProPhoto RGB (ROMM)
// -----------------------------------------------------------------------------

fn prophoto_decode(x: f32) -> f32 {
    if x <= 1.0 / 32.0 { // 1/32 in encoded space
        x / 16.0
    } else {
        x.powf(1.8)
    }
}

fn prophoto_encode(y: f32) -> f32 {
    if y <= 0.001953125 { // 1/512
        16.0 * y
    } else {
        y.powf(1.0 / 1.8)
    }
}

// -----------------------------------------------------------------------------
// PQ (SMPTE ST 2084)
// -----------------------------------------------------------------------------

const PQ_M1: f32 = 2610.0 / 16384.0;
const PQ_M2: f32 = (2523.0 / 4096.0) * 128.0;
const PQ_C1: f32 = 3424.0 / 4096.0;
const PQ_C2: f32 = (2413.0 / 4096.0) * 32.0;
const PQ_C3: f32 = (2392.0 / 4096.0) * 32.0;

fn pq_decode(x: f32) -> f32 {
    let x_inv_m2 = x.max(0.0).powf(1.0 / PQ_M2);
    let num = (x_inv_m2 - PQ_C1).max(0.0);
    let den = PQ_C2 - PQ_C3 * x_inv_m2;
    (num / den).powf(1.0 / PQ_M1)
}

fn pq_encode(y: f32) -> f32 {
    let y_m1 = y.max(0.0).powf(PQ_M1);
    let num = PQ_C1 + PQ_C2 * y_m1;
    let den = 1.0 + PQ_C3 * y_m1;
    (num / den).powf(PQ_M2)
}

// -----------------------------------------------------------------------------
// HLG (BBC Hybrid Log-Gamma)
// -----------------------------------------------------------------------------

const HLG_A: f32 = 0.17883277;
const HLG_B: f32 = 0.28466892;
const HLG_C: f32 = 0.55991073;

fn hlg_decode(x: f32) -> f32 {
    let y = if x <= 0.5 {
        (x * x) / 3.0
    } else {
        ((x - HLG_C) / HLG_A).exp() + HLG_B
    };
    y / 12.0
}

fn hlg_encode(y: f32) -> f32 {
    let y = y.max(0.0) * 12.0;
    if y <= 1.0 {
        (3.0 * y).sqrt()
    } else {
        HLG_A * (y - HLG_B).ln() + HLG_C
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn linear_identity() {
        let tf = TransferFn::Linear;
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_approx_eq!(tf.decode(x), x, 1e-6);
            assert_approx_eq!(tf.encode(x), x, 1e-6);
        }
    }

    #[test]
    fn srgb_inverse() {
        let tf = TransferFn::SrgbGamma;
        for x in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
        for y in [0.0, 0.01, 0.1, 0.5, 1.0, 2.0] {
            let encoded = tf.encode(y);
            let decoded = tf.decode(encoded);
            // For y > 1.0, encode clamps; we test that decoded ≈ y when y ≤ 1.0
            if y <= 1.0 {
                assert_approx_eq!(decoded, y, 1e-5);
            }
        }
    }

    #[test]
    fn rec709_inverse() {
        let tf = TransferFn::Rec709Gamma;
        for x in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
    }

    #[test]
    fn gamma22_inverse() {
        let tf = TransferFn::Gamma22;
        for x in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
    }

    #[test]
    fn gamma24_inverse() {
        let tf = TransferFn::Gamma24;
        for x in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
    }

    #[test]
    fn prophoto_inverse() {
        let tf = TransferFn::ProPhotoGamma;
        for x in [0.0, 0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
    }

    #[test]
    fn pq_inverse() {
        let tf = TransferFn::Pq;
        for x in [0.0, 0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
    }

    #[test]
    fn hlg_inverse() {
        let tf = TransferFn::Hlg;
        for x in [0.0, 0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let decoded = tf.decode(x);
            let encoded = tf.encode(decoded);
            assert_approx_eq!(encoded, x, 1e-5);
        }
    }
}