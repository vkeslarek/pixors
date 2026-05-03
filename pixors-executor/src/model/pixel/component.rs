//! Numeric component trait for pixel channels.

use bytemuck::{Pod, Zeroable};

/// Trait for numeric types that can represent a pixel channel.
///
/// All components must be copyable, have a fixed bit depth, and be convertible
/// to/from `f32` with appropriate scaling.
pub trait Component: Copy + 'static + Pod + Zeroable {
    const ZERO: Self;

    const ONE: Self;

    /// Maximum value in normalized `f32` (255 for u8, 65535 for u16, 1.0 for floats).
    const MAX_ONE_F32: f32;

    /// Converts the component to normalized `f32` in range [0, 1] for unsigned integers,
    /// or direct value for floats.
    fn to_f32(self) -> f32;

    /// Converts a normalized `f32` value to the component, clamping to the valid range.
    fn from_f32_clamped(v: f32) -> Self;
}

// -----------------------------------------------------------------------------
// Implementations
// -----------------------------------------------------------------------------

impl Component for u8 {
    const ZERO: u8 = 0;
    const ONE: u8 = 255;
    const MAX_ONE_F32: f32 = 255.0;

    fn to_f32(self) -> f32 {
        self as f32 / Self::MAX_ONE_F32
    }

    fn from_f32_clamped(v: f32) -> Self {
        let clamped = v.clamp(0.0, 1.0);
        (clamped * Self::MAX_ONE_F32).round() as u8
    }
}

impl Component for u16 {
    const ZERO: u16 = 0;
    const ONE: u16 = 65535;
    const MAX_ONE_F32: f32 = 65535.0;

    fn to_f32(self) -> f32 {
        self as f32 / Self::MAX_ONE_F32
    }

    fn from_f32_clamped(v: f32) -> Self {
        let clamped = v.clamp(0.0, 1.0);
        (clamped * Self::MAX_ONE_F32).round() as u16
    }
}

impl Component for half::f16 {
    const ZERO: half::f16 = half::f16::from_bits(0);
    const ONE: half::f16 = half::f16::from_bits(0x3C00);
    const MAX_ONE_F32: f32 = 1.0; // 1.0 in f16

    fn to_f32(self) -> f32 {
        // Call the primitive f16's to_f32 method
        half::f16::to_f32(self)
    }

    fn from_f32_clamped(v: f32) -> Self {
        // f16 can represent values outside [0,1]; we still clamp for consistency.
        half::f16::from_f32(v.clamp(0.0, 1.0))
    }
}

impl Component for f32 {
    const ZERO: f32 = 0.0;
    const ONE: f32 = 1.0;
    const MAX_ONE_F32: f32 = 1.0;

    fn to_f32(self) -> f32 {
        self
    }

    fn from_f32_clamped(v: f32) -> Self {
        v.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_approx_eq;

    #[test]
    fn u8_conversion() {
        assert_eq!(u8::MAX_ONE_F32, 255.0);
        assert_eq!(0u8.to_f32(), 0.0);
        assert_approx_eq!(255u8.to_f32(), 1.0);
        assert_approx_eq!(128u8.to_f32(), 128.0 / 255.0);

        assert_eq!(u8::from_f32_clamped(0.0), 0);
        assert_eq!(u8::from_f32_clamped(1.0), 255);
        assert_eq!(u8::from_f32_clamped(0.5), 128); // 127.5 rounds to 128
        assert_eq!(u8::from_f32_clamped(-0.1), 0);
        assert_eq!(u8::from_f32_clamped(1.1), 255);
    }

    #[test]
    fn u16_conversion() {
        assert_eq!(u16::MAX_ONE_F32, 65535.0);
        assert_eq!(0u16.to_f32(), 0.0);
        assert_approx_eq!(65535u16.to_f32(), 1.0);

        assert_eq!(u16::from_f32_clamped(0.0), 0);
        assert_eq!(u16::from_f32_clamped(1.0), 65535);
        assert_eq!(u16::from_f32_clamped(0.5), 32768); // 32767.5 rounds to 32768
    }

    #[test]
    fn f16_conversion() {
        assert_eq!(half::f16::MAX_ONE_F32, 1.0);
        let val = half::f16::from_f32(0.5);
        assert_approx_eq!(val.to_f32(), 0.5, 1e-3);

        let clamped = half::f16::from_f32_clamped(1.5);
        assert!(clamped.to_f32() <= 1.0);
    }

    #[test]
    fn f32_conversion() {
        assert_eq!(f32::MAX_ONE_F32, 1.0);
        assert_eq!(0.7f32.to_f32(), 0.7);
        assert_eq!(f32::from_f32_clamped(1.5), 1.0);
        assert_eq!(f32::from_f32_clamped(-0.2), 0.0);
    }
}
