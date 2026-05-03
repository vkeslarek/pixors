use bytemuck::Pod;
use wide::f32x4;

// ---------------------------------------------------------------------------
// Component trait
// ---------------------------------------------------------------------------

mod component;
pub use component::Component;

// ---------------------------------------------------------------------------
// Alpha policy — runtime param controlling premultiplication on pack
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

mod accumulator;
mod format;
mod gray;
pub mod meta;
mod pack;
mod rgb;
mod rgba;

pub use accumulator::PixelAccumulator;
pub use format::PixelFormat;
pub use gray::{Gray, GrayAlpha};
pub use meta::PixelMeta;
pub use rgb::Rgb;
pub use rgba::Rgba;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::{bytes_of, cast_slice};

    #[test]
    fn alpha_policy_is_copy() {
        let p = AlphaPolicy::PremultiplyOnPack;
        let _q = p;
        assert_eq!(p, AlphaPolicy::PremultiplyOnPack);
    }

    #[test]
    fn rgba_repr_c() {
        let pixel = Rgba::new(1u8, 2u8, 3u8, 4u8);
        let bytes = bytes_of(&pixel);
        assert_eq!(bytes.len(), 4);
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[1], 2);
        assert_eq!(bytes[2], 3);
        assert_eq!(bytes[3], 4);
    }

    #[test]
    fn rgb_repr_c() {
        let pixel = Rgb::new(10u8, 20u8, 30u8);
        let bytes = bytes_of(&pixel);
        assert_eq!(bytes.len(), 3);
        assert_eq!(bytes[0], 10);
        assert_eq!(bytes[1], 20);
        assert_eq!(bytes[2], 30);
    }

    #[test]
    fn gray_repr_c() {
        let pixel = Gray::new(42u8);
        let bytes = bytes_of(&pixel);
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 42);
    }

    #[test]
    fn gray_alpha_repr_c() {
        let pixel = GrayAlpha::new(100u8, 200u8);
        let bytes = bytes_of(&pixel);
        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0], 100);
        assert_eq!(bytes[1], 200);
    }

    #[test]
    fn bytemuck_cast() {
        let rgba = Rgba::new(1u8, 2u8, 3u8, 4u8);
        let arr = [rgba];
        let slice = cast_slice::<Rgba<u8>, u8>(&arr);
        assert_eq!(slice, &[1, 2, 3, 4]);
    }

    #[test]
    fn rgba_white_black() {
        let w = Rgba::<u8>::white();
        assert_eq!(w.r, 255);
        assert_eq!(w.a, 255);
        let b = Rgba::<u8>::black();
        assert_eq!(b.r, 0);
        assert_eq!(b.a, 255);
    }

    #[test]
    fn pixel_roundtrip_f16() {
        use half::f16;
        let orig = Rgba {
            r: f16::from_f32(0.5),
            g: f16::from_f32(0.3),
            b: f16::from_f32(0.2),
            a: f16::ONE,
        };
        let unpacked = orig.unpack();
        let repacked = Rgba::<f16>::pack_one(unpacked, AlphaPolicy::Straight);
        assert!((orig.r.to_f32() - repacked.r.to_f32()).abs() < 1e-3);
    }

    #[test]
    fn pixel_roundtrip_u8() {
        let orig: [u8; 4] = [128, 64, 32, 255];
        let unpacked = orig.unpack();
        let repacked = <[u8; 4]>::pack_one(unpacked, AlphaPolicy::Straight);
        assert_eq!(orig, repacked);
    }
}
