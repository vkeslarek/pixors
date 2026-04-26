//! Operation trait stub — Phase 3 prep.
//!
//! Defines the shape for future tile-level image operations.
//! Not wired into the compositor yet.

use crate::pixel::Rgba;
use half::f16;

pub enum OpScope {
    /// Preview at a specific MIP level (fast, approximate).
    Preview { mip_level: u32 },
    /// Full-quality apply on MIP 0 (background task).
    Apply,
}

pub trait Operation: Send + Sync {
    fn name(&self) -> &str;
    fn apply_tile(&self, scope: OpScope, src: &[Rgba<f16>], dst: &mut [Rgba<f16>]);
}

// Example impl — validates the shape.
pub struct Brightness { pub gain: f32 }

impl Operation for Brightness {
    fn name(&self) -> &str { "brightness" }
    fn apply_tile(&self, _scope: OpScope, src: &[Rgba<f16>], dst: &mut [Rgba<f16>]) {
        let g = f16::from_f32(self.gain);
        for (s, d) in src.iter().zip(dst.iter_mut()) {
            *d = Rgba::new(s.r * g, s.g * g, s.b * g, s.a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_identity() {
        let op = Brightness { gain: 1.0 };
        let src = [Rgba::new(f16::ONE, f16::from_f32(0.5), f16::ZERO, f16::ONE)];
        let mut dst = [Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO)];
        op.apply_tile(OpScope::Apply, &src, &mut dst);
        assert!((dst[0].r.to_f32() - 1.0).abs() < 1e-3);
        assert!((dst[0].g.to_f32() - 0.5).abs() < 1e-3);
    }
}
