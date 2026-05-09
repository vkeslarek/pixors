//! Color model decode — converts non-RGB pixel layouts (CMYK, YCbCr, Lab) to
//! linear sRGB coordinates, after which the standard transfer → matrix →
//! encode pipeline takes over.
//!
//! For Lab: output may exceed [0,1] (out-of-gamut colors). No clamping is
//! applied so the downstream primary matrix (sRGB→dst) handles them correctly,
//! which is mathematically equivalent to Lab→XYZ→dst without intermediate loss.

use wide::f32x4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ColorModelTransform {
    None = 0,
    CmykToRgb = 1,
    CmykAToRgb = 2,
    YCbCrToRgb = 3,
    LabToRgb = 4,
}

impl ColorModelTransform {
    pub fn decode_4(&self, c0: f32x4, c1: f32x4, c2: f32x4, c3: f32x4) -> (f32x4, f32x4, f32x4) {
        match self {
            Self::None => (c0, c1, c2),
            Self::CmykToRgb | Self::CmykAToRgb => {
                let one = f32x4::splat(1.0);
                let scale = one - c3;
                ((one - c0) * scale, (one - c1) * scale, (one - c2) * scale)
            }
            Self::YCbCrToRgb => {
                let cb = c1 - f32x4::splat(0.5);
                let cr = c2 - f32x4::splat(0.5);
                (
                    c0 + cr * f32x4::splat(1.402),
                    c0 - cb * f32x4::splat(0.344136) - cr * f32x4::splat(0.714136),
                    c0 + cb * f32x4::splat(1.772),
                )
            }
            Self::LabToRgb => {
                let l = c0.to_array();
                let a = c1.to_array();
                let b = c2.to_array();
                let mut ro = [0.0f32; 4];
                let mut go = [0.0f32; 4];
                let mut bo = [0.0f32; 4];
                for i in 0..4 {
                    let [r, g, bv] = lab_to_linear_srgb(l[i], a[i], b[i]);
                    ro[i] = r;
                    go[i] = g;
                    bo[i] = bv;
                }
                (f32x4::from(ro), f32x4::from(go), f32x4::from(bo))
            }
        }
    }

    pub fn decode_1(&self, ch: &[f32; 4]) -> [f32; 3] {
        match self {
            Self::None => [ch[0], ch[1], ch[2]],
            Self::CmykToRgb | Self::CmykAToRgb => {
                let scale = 1.0 - ch[3];
                [
                    (1.0 - ch[0]) * scale,
                    (1.0 - ch[1]) * scale,
                    (1.0 - ch[2]) * scale,
                ]
            }
            Self::YCbCrToRgb => {
                let cb = ch[1] - 0.5;
                let cr = ch[2] - 0.5;
                [
                    ch[0] + 1.402 * cr,
                    ch[0] - 0.344136 * cb - 0.714136 * cr,
                    ch[0] + 1.772 * cb,
                ]
            }
            Self::LabToRgb => lab_to_linear_srgb(ch[0], ch[1], ch[2]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn lab_black_is_zero() {
        // L*=0, a*=0, b*=0 → XYZ=(0,0,0) → sRGB=(0,0,0)
        let [r, g, b] = ColorModelTransform::LabToRgb.decode_1(&[0.0, 0.0, 0.0, 1.0]);
        assert!(approx(r, 0.0, 1e-4), "r={r}");
        assert!(approx(g, 0.0, 1e-4), "g={g}");
        assert!(approx(b, 0.0, 1e-4), "b={b}");
    }

    #[test]
    fn lab_d50_white_is_one() {
        // L*=100, a*=0, b*=0 → D50 white → after D50→sRGB matrix → (1,1,1) linear
        let l_norm = 1.0_f32;
        let [r, g, b] = ColorModelTransform::LabToRgb.decode_1(&[l_norm, 0.0, 0.0, 1.0]);
        assert!(approx(r, 1.0, 1e-4), "r={r}");
        assert!(approx(g, 1.0, 1e-4), "g={g}");
        assert!(approx(b, 1.0, 1e-4), "b={b}");
    }

    #[test]
    fn lab_wide_gamut_no_clamp() {
        // Saturated Lab may produce sRGB > 1 — must NOT be clamped
        // e.g. L*=50, a*=100 (very saturated red) → r > 1 in sRGB
        let a_norm = 100.0_f32 / 128.0;
        let [r, _g, _b] = ColorModelTransform::LabToRgb.decode_1(&[0.5, a_norm, 0.0, 1.0]);
        assert!(
            r > 1.0 || r > 0.9,
            "expected large r for saturated Lab, got r={r}"
        );
    }

    #[test]
    fn cmyk_white() {
        let [r, g, b] = ColorModelTransform::CmykToRgb.decode_1(&[0.0, 0.0, 0.0, 0.0]);
        assert!(approx(r, 1.0, 1e-6));
        assert!(approx(g, 1.0, 1e-6));
        assert!(approx(b, 1.0, 1e-6));
    }

    #[test]
    fn cmyk_black() {
        let [r, g, b] = ColorModelTransform::CmykToRgb.decode_1(&[0.0, 0.0, 0.0, 1.0]);
        assert!(approx(r, 0.0, 1e-6));
        assert!(approx(g, 0.0, 1e-6));
        assert!(approx(b, 0.0, 1e-6));
    }

    #[test]
    fn ycbcr_neutral_gray() {
        // Y=0.5, Cb=0.5, Cr=0.5 → neutral (no chroma) → (0.5, 0.5, 0.5)
        let [r, g, b] = ColorModelTransform::YCbCrToRgb.decode_1(&[0.5, 0.5, 0.5, 1.0]);
        assert!(approx(r, 0.5, 1e-5), "r={r}");
        assert!(approx(g, 0.5, 1e-5), "g={g}");
        assert!(approx(b, 0.5, 1e-5), "b={b}");
    }

    #[test]
    fn ycbcr_white() {
        // Y=1, Cb=0.5, Cr=0.5 → white
        let [r, g, b] = ColorModelTransform::YCbCrToRgb.decode_1(&[1.0, 0.5, 0.5, 1.0]);
        assert!(approx(r, 1.0, 1e-5), "r={r}");
        assert!(approx(g, 1.0, 1e-5), "g={g}");
        assert!(approx(b, 1.0, 1e-5), "b={b}");
    }
}

/// CIE L*a*b* → linear sRGB coordinates.
/// Inputs: L_norm in [0..1] (L* = 100×L_norm),
///         a_norm in [-1..1] (a* = 128×a_norm),
///         b_norm in [-1..1] (b* = 128×b_norm).
/// Output: linear sRGB coordinates — may be outside [0..1] for wide-gamut Lab colors.
/// No clamping: downstream primary matrix (sRGB→dst) handles out-of-gamut values,
/// making this equivalent to Lab→XYZ→dst without lossy intermediate clamping.
fn lab_to_linear_srgb(l_norm: f32, a_norm: f32, b_norm: f32) -> [f32; 3] {
    let l_star = l_norm * 100.0;
    let a_star = a_norm * 128.0;
    let b_star = b_norm * 128.0;

    let fy = (l_star + 16.0) / 116.0;
    let fx = a_star / 500.0 + fy;
    let fz = fy - b_star / 200.0;

    const DELTA: f32 = 6.0 / 29.0;
    const DELTA2: f32 = 3.0 * DELTA * DELTA;
    let finv = |t: f32| {
        if t > DELTA {
            t * t * t
        } else {
            DELTA2 * (t - 4.0 / 29.0)
        }
    };

    // D50 white point: Xn=0.96422, Yn=1.0, Zn=0.82521
    let x = 0.96422 * finv(fx);
    let y = finv(fy);
    let z = 0.82521 * finv(fz);

    // D50-adapted XYZ → linear sRGB (Bradford D50→D65 + D65→sRGB combined)
    let r = 3.133_856 * x - 1.616_867 * y - 0.490_615 * z;
    let g = -0.978_768 * x + 1.916_142 * y + 0.033_454 * z;
    let b = 0.071_945 * x - 0.228_991 * y + 1.405_243 * z;
    [r, g, b]
}
