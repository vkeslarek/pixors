//! Pixel types and traits.
//!
//! Provides compile-time information about the pixel's component type,
//! channel count, alpha presence, and layout, as well as concrete pixel structs
//! with fixed memory layout.

mod component;
pub use component::Component;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

// --- Concrete Pixel Types ---

/// RGBA pixel with four components (red, green, blue, alpha).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba<T: Component> {
    pub r: T,
    pub g: T,
    pub b: T,
    pub a: T,
}

impl<T: Component> Rgba<T> {
    /// Creates a new RGBA pixel.
    pub const fn new(r: T, g: T, b: T, a: T) -> Self {
        Self { r, g, b, a }
    }

    /// All-zero RGBA pixel (0, 0, 0, 0).
    pub const ZERO: Self = Self { r: T::ZERO, g: T::ZERO, b: T::ZERO, a: T::ZERO };

    /// All-one RGBA pixel (1, 1, 1, 1).
    pub const ONE: Self = Self { r: T::ONE, g: T::ONE, b: T::ONE, a: T::ONE };

    /// Creates an opaque black pixel (0, 0, 0, 1).
    pub fn black() -> Self {
        Self::new(
            T::ZERO,
            T::ZERO,
            T::ZERO,
            T::ONE,
        )
    }

    /// Creates an opaque white pixel (1, 1, 1, 1).
    pub fn white() -> Self {
        Self::new(
            T::ONE,
            T::ONE,
            T::ONE,
            T::ONE,
        )
    }
}

/// RGB pixel with three components (red, green, blue).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb<T: Component> {
    pub r: T,
    pub g: T,
    pub b: T,
}

impl<T: Component> Rgb<T> {
    /// Creates a new RGB pixel.
    pub const fn new(r: T, g: T, b: T) -> Self {
        Self { r, g, b }
    }
}

/// Grayscale pixel with one component.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gray<T: Component> {
    pub v: T,
}

impl<T: Component> Gray<T> {
    /// Creates a new grayscale pixel.
    pub const fn new(v: T) -> Self {
        Self { v }
    }
}

/// Grayscale pixel with alpha.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrayAlpha<T: Component> {
    pub v: T,
    pub a: T,
}

impl<T: Component> GrayAlpha<T> {
    /// Creates a new grayscale+alpha pixel.
    pub const fn new(v: T, a: T) -> Self {
        Self { v, a }
    }
}

// -----------------------------------------------------------------------------
// `bytemuck` safety implementations
// -----------------------------------------------------------------------------

// `Rgba<T>` is `Pod` and `Zeroable` if `T` is `Pod` and `Zeroable`.
// Since `Component` requires `Pod + Zeroable`, we can implement generically.

unsafe impl<T: Component> Pod for Rgba<T> {}
unsafe impl<T: Component> Zeroable for Rgba<T> {}
unsafe impl<T: Component> Pod for Rgb<T> {}
unsafe impl<T: Component> Zeroable for Rgb<T> {}
unsafe impl<T: Component> Pod for Gray<T> {}
unsafe impl<T: Component> Zeroable for Gray<T> {}
unsafe impl<T: Component> Pod for GrayAlpha<T> {}
unsafe impl<T: Component> Zeroable for GrayAlpha<T> {}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests_types {
    use super::*;
    use bytemuck::{bytes_of, cast_slice};

    #[test]
    fn rgba_repr_c() {
        // Ensure no padding between fields.
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
        // u8
        let white_u8 = Rgba::<u8>::white();
        assert_eq!(white_u8.r, 255);
        assert_eq!(white_u8.g, 255);
        assert_eq!(white_u8.b, 255);
        assert_eq!(white_u8.a, 255);
        let black_u8 = Rgba::<u8>::black();
        assert_eq!(black_u8.r, 0);
        assert_eq!(black_u8.g, 0);
        assert_eq!(black_u8.b, 0);
        assert_eq!(black_u8.a, 255);

        // u16
        let white_u16 = Rgba::<u16>::white();
        assert_eq!(white_u16.r, 65535);
        assert_eq!(white_u16.g, 65535);
        assert_eq!(white_u16.b, 65535);
        assert_eq!(white_u16.a, 65535);
        let black_u16 = Rgba::<u16>::black();
        assert_eq!(black_u16.r, 0);
        assert_eq!(black_u16.g, 0);
        assert_eq!(black_u16.b, 0);
        assert_eq!(black_u16.a, 65535);

        // f32
        let white_f32 = Rgba::<f32>::white();
        assert_eq!(white_f32.r, 1.0);
        assert_eq!(white_f32.g, 1.0);
        assert_eq!(white_f32.b, 1.0);
        assert_eq!(white_f32.a, 1.0);
        let black_f32 = Rgba::<f32>::black();
        assert_eq!(black_f32.r, 0.0);
        assert_eq!(black_f32.g, 0.0);
        assert_eq!(black_f32.b, 0.0);
        assert_eq!(black_f32.a, 1.0);

        // f16
        let white_f16 = Rgba::<half::f16>::white();
        assert_eq!(white_f16.r, half::f16::from_f32(1.0));
        assert_eq!(white_f16.g, half::f16::from_f32(1.0));
        assert_eq!(white_f16.b, half::f16::from_f32(1.0));
        assert_eq!(white_f16.a, half::f16::from_f32(1.0));
        let black_f16 = Rgba::<half::f16>::black();
        assert_eq!(black_f16.r, half::f16::from_f32(0.0));
        assert_eq!(black_f16.g, half::f16::from_f32(0.0));
        assert_eq!(black_f16.b, half::f16::from_f32(0.0));
        assert_eq!(black_f16.a, half::f16::from_f32(1.0));
    }
}
// --- Pixel Traits and Layouts ---

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

/// Pixel format for binary transmission.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    /// RGBA8, 4 bytes per pixel.
    Rgba8,
    /// ARGB32, 4 bytes per pixel (u32).
    Argb32,
}

impl PixelFormat {
    /// Returns bytes per pixel.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgba8 => 4,
            PixelFormat::Argb32 => 4,
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests_pixel {
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
