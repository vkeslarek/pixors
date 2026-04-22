//! Concrete pixel structs with fixed memory layout.

use super::Component;
use bytemuck::{Pod, Zeroable};

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
mod tests {
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