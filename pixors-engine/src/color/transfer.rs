//! Transfer functions (OETF/EOTF) for color spaces.

/// Invertible mapping between encoded and linear light values.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TransferFn {
    Linear,
    SrgbGamma,
    Rec709Gamma,
    Gamma22,
    Gamma24,
    Gamma26,
    ProPhotoGamma,
    Pq,
    Hlg,
}

impl TransferFn {
    /// Decode one encoded `f32` value to linear light.
    pub fn decode(self, x: f32) -> f32 {
        match self {
            Self::Linear => x,
            Self::SrgbGamma => srgb_decode(x),
            Self::Rec709Gamma => rec709_decode(x),
            Self::Gamma22 => x.powf(2.2),
            Self::Gamma24 => x.powf(2.4),
            Self::Gamma26 => x.powf(2.6),
            Self::ProPhotoGamma => prophoto_decode(x),
            Self::Pq => pq_decode(x),
            Self::Hlg => hlg_decode(x),
        }
    }

    /// Encode one linear `f32` value to non-linear representation.
    pub fn encode(self, y: f32) -> f32 {
        match self {
            Self::Linear => y,
            Self::SrgbGamma => srgb_encode(y),
            Self::Rec709Gamma => rec709_encode(y),
            Self::Gamma22 => y.max(0.0).powf(1.0 / 2.2),
            Self::Gamma24 => y.max(0.0).powf(1.0 / 2.4),
            Self::Gamma26 => y.max(0.0).powf(1.0 / 2.6),
            Self::ProPhotoGamma => prophoto_encode(y),
            Self::Pq => pq_encode(y),
            Self::Hlg => hlg_encode(y),
        }
    }

    pub const fn is_linear(self) -> bool {
        matches!(self, Self::Linear)
    }

    /// Map a gamma decode value to a known transfer function, or return `None`.
    pub fn from_gamma(g: f32) -> Option<Self> {
        if (g - 1.0 / 2.2).abs() < 0.01 {
            Some(Self::Gamma22)
        } else if (g - 1.0 / 2.4).abs() < 0.01 {
            Some(Self::Gamma24)
        } else if (g - 1.0 / 2.2).abs() < 0.05 {
            Some(Self::Gamma22)
        } else if (g - 1.0).abs() < 0.01 {
            Some(Self::Linear)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Transfer function implementations
// ---------------------------------------------------------------------------

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

fn prophoto_decode(x: f32) -> f32 {
    if x <= 1.0 / 32.0 {
        x / 16.0
    } else {
        x.powf(1.8)
    }
}
fn prophoto_encode(y: f32) -> f32 {
    if y <= 0.001953125 {
        16.0 * y
    } else {
        y.powf(1.0 / 1.8)
    }
}

const PQ_M1: f32 = 2610.0 / 16384.0;
const PQ_M2: f32 = (2523.0 / 4096.0) * 128.0;
const PQ_C1: f32 = 3424.0 / 4096.0;
const PQ_C2: f32 = (2413.0 / 4096.0) * 32.0;
const PQ_C3: f32 = (2392.0 / 4096.0) * 32.0;

fn pq_decode(x: f32) -> f32 {
    let xm2 = x.max(0.0).powf(1.0 / PQ_M2);
    ((xm2 - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * xm2)).powf(1.0 / PQ_M1)
}
fn pq_encode(y: f32) -> f32 {
    let ym1 = y.max(0.0).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * ym1) / (1.0 + PQ_C3 * ym1)).powf(PQ_M2)
}

const HLG_A: f32 = 0.17883277;
const HLG_B: f32 = 0.28466892;
const HLG_C: f32 = 0.559_910_7;

fn hlg_decode(x: f32) -> f32 {
    // HLG EOTF: signal [0,1] → scene-referred linear [0,1]
    if x <= 0.5 {
        (x * x) / 3.0
    } else {
        (((x - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}
fn hlg_encode(y: f32) -> f32 {
    // HLG OETF: scene-referred linear [0,1] → signal [0,1]
    let y = y.max(0.0);
    if y <= 1.0 / 12.0 {
        (3.0 * y).sqrt()
    } else {
        HLG_A * (12.0 * y - HLG_B).ln() + HLG_C
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    fn assert_inverse(tf: TransferFn) {
        for x in [0.0_f32, 0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            assert_approx_eq!(tf.encode(tf.decode(x)), x, 1e-4);
            assert_approx_eq!(tf.decode(tf.encode(x)), x, 1e-4);
        }
    }

    #[test]
    fn linear_inverse() {
        assert_inverse(TransferFn::Linear);
    }
    #[test]
    fn srgb_inverse() {
        assert_inverse(TransferFn::SrgbGamma);
    }
    #[test]
    fn rec709_inverse() {
        assert_inverse(TransferFn::Rec709Gamma);
    }
    #[test]
    fn gamma22_inverse() {
        assert_inverse(TransferFn::Gamma22);
    }
    #[test]
    fn gamma24_inverse() {
        assert_inverse(TransferFn::Gamma24);
    }
    #[test]
    fn gamma26_inverse() {
        assert_inverse(TransferFn::Gamma26);
    }
    #[test]
    fn prophoto_inverse() {
        assert_inverse(TransferFn::ProPhotoGamma);
    }
    #[test]
    fn pq_inverse() {
        assert_inverse(TransferFn::Pq);
    }
    #[test]
    fn hlg_inverse() {
        assert_inverse(TransferFn::Hlg);
    }
}
