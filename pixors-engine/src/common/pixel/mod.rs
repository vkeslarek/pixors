use bytemuck::Pod;
use serde::{Deserialize, Serialize};
use wide::f32x4;

// ---------------------------------------------------------------------------
// Component trait
// ---------------------------------------------------------------------------

mod component;
pub use component::Component;

// ---------------------------------------------------------------------------
// Alpha policy — runtime param controlling premultiplication on pack
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaPolicy {
    PremultiplyOnPack,
    Straight,
    /// Destination has no alpha channel; RGB is premultiplied, alpha discarded on pack.
    OpaqueDrop,
}

// ---------------------------------------------------------------------------
// Pixel trait — unified pack/unpack between concrete type ↔ [f32;4]
// ---------------------------------------------------------------------------

/// Bidirectional conversion between a concrete pixel type and the `[f32;4]`
/// intermediate RGBA representation used by the conversion pipeline.
///
/// `unpack`: pixel → straight linear `[r, g, b, a]` (source side, unpremuls if needed).
/// `pack`: post-matrix+encode `[r, g, b, a]` → pixel (destination side).
pub trait Pixel: Copy + Pod {
    fn unpack(self) -> [f32; 4];

    fn unpack_x4(s: &[Self]) -> (f32x4, f32x4, f32x4, f32x4) {
        let mut r = [0.0_f32; 4];
        let mut g = [0.0_f32; 4];
        let mut b = [0.0_f32; 4];
        let mut a = [0.0_f32; 4];
        for i in 0..4 {
            let [rr, gg, bb, aa] = s[i].unpack();
            r[i] = rr;
            g[i] = gg;
            b[i] = bb;
            a[i] = aa;
        }
        (
            f32x4::from(r),
            f32x4::from(g),
            f32x4::from(b),
            f32x4::from(a),
        )
    }

    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, mode: AlphaPolicy, out: &mut [Self]);
    fn pack_one(rgba: [f32; 4], mode: AlphaPolicy) -> Self;
}

// ---------------------------------------------------------------------------
// Sub-modules
// ---------------------------------------------------------------------------

pub mod cmyk;
pub mod format;
pub mod gray;
pub mod lab;
pub mod meta;
mod pack;
pub mod rgb;
pub mod rgba;
pub mod ycbcr;

pub use cmyk::{Cmyk, CmykA};
pub use format::PixelFormat;
pub use gray::{Gray, GrayAlpha};
pub use lab::Lab;
pub use meta::PixelMeta;
pub use rgb::Rgb;
pub use rgba::Rgba;
pub use ycbcr::YCbCr;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_policy_is_copy() {
        let p = AlphaPolicy::PremultiplyOnPack;
        let _q = p;
        assert_eq!(p, AlphaPolicy::PremultiplyOnPack);
    }

    #[test]
    fn pixel_roundtrip_u8() {
        let orig: [u8; 4] = [128, 64, 32, 255];
        let unpacked = orig.unpack();
        let repacked = <[u8; 4]>::pack_one(unpacked, AlphaPolicy::Straight);
        assert_eq!(orig, repacked);
    }
}
