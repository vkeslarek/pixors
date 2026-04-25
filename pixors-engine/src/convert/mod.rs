//! Color and pixel conversion pipelines.

mod premultiply;
pub(crate) mod simd;

pub use premultiply::{premultiply, unpremultiply};

use crate::pixel::Rgba;
use half::f16;

/// Pack linear RGB + straight alpha into premultiplied Rgba<f16>.
#[inline(always)]
pub(crate) fn pack_rgba_premul(rgb: [f32; 3], a: f32) -> Rgba<f16> {
    Rgba {
        r: f16::from_f32(rgb[0] * a),
        g: f16::from_f32(rgb[1] * a),
        b: f16::from_f32(rgb[2] * a),
        a: f16::from_f32(a),
    }
}
