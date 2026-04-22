//! Pixel trait and layout.

use super::{Component, types::{Rgba, Rgb, Gray, GrayAlpha}};
use bytemuck::{Pod, Zeroable};

/// Layout of concrete pixel types.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PixelLayout {
    /// RGB layout.
    Rgb,
    /// RGBA layout.
    Rgba,
    /// Grayscale layout.
    Gray,
    /// Grayscale with alpha.
    GrayAlpha,
}

/// Trait for pixel types.
///
/// Provides compile‑time information about the pixel's component type,
/// channel count, alpha presence, and layout.
pub trait Pixel: Copy + 'static + Pod + Zeroable {
    /// Component type of each channel.
    type Component: Component;

    /// Number of channels.
    const CHANNELS: u8;

    /// Whether the pixel has an alpha channel.
    const HAS_ALPHA: bool;

    /// Pixel layout (Rgb, Rgba, Gray, GrayAlpha).
    const LAYOUT: PixelLayout;
}

// -----------------------------------------------------------------------------
// Implementations for concrete pixel types
// -----------------------------------------------------------------------------

impl<T: Component> Pixel for Rgba<T> {
    type Component = T;
    const CHANNELS: u8 = 4;
    const HAS_ALPHA: bool = true;
    const LAYOUT: PixelLayout = PixelLayout::Rgba;
}

impl<T: Component> Pixel for Rgb<T> {
    type Component = T;
    const CHANNELS: u8 = 3;
    const HAS_ALPHA: bool = false;
    const LAYOUT: PixelLayout = PixelLayout::Rgb;
}

impl<T: Component> Pixel for Gray<T> {
    type Component = T;
    const CHANNELS: u8 = 1;
    const HAS_ALPHA: bool = false;
    const LAYOUT: PixelLayout = PixelLayout::Gray;
}

impl<T: Component> Pixel for GrayAlpha<T> {
    type Component = T;
    const CHANNELS: u8 = 2;
    const HAS_ALPHA: bool = true;
    const LAYOUT: PixelLayout = PixelLayout::GrayAlpha;
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;


    #[test]
    fn pixel_traits() {
        // Rgba
        assert_eq!(Rgba::<u8>::CHANNELS, 4);
        assert!(Rgba::<u8>::HAS_ALPHA);
        assert_eq!(Rgba::<u8>::LAYOUT, PixelLayout::Rgba);

        // Rgb
        assert_eq!(Rgb::<f32>::CHANNELS, 3);
        assert!(!Rgb::<f32>::HAS_ALPHA);
        assert_eq!(Rgb::<f32>::LAYOUT, PixelLayout::Rgb);

        // Gray
        assert_eq!(Gray::<u16>::CHANNELS, 1);
        assert!(!Gray::<u16>::HAS_ALPHA);
        assert_eq!(Gray::<u16>::LAYOUT, PixelLayout::Gray);

        // GrayAlpha
        assert_eq!(GrayAlpha::<f16>::CHANNELS, 2);
        assert!(GrayAlpha::<f16>::HAS_ALPHA);
        assert_eq!(GrayAlpha::<f16>::LAYOUT, PixelLayout::GrayAlpha);
    }
}