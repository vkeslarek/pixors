//! Typed image: compile‑time pixel type + runtime color/alpha metadata.

use crate::error::Error;
use crate::color::ColorSpace;
use crate::pixel::Pixel;
use super::AlphaMode;


/// Typed (compile‑time‑parameterized) image representation.
///
/// The pixors-engine processes `TypedImage<P>`; the canonical working image is
/// `TypedImage<Rgba<f16>>` in ACEScg premul.
#[derive(Debug, Clone)]
pub struct TypedImage<P: Pixel> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Interleaved pixel buffer, exactly `width * height` entries.
    pub pixels: Vec<P>,

    /// Color space of the stored pixels.
    pub color_space: ColorSpace,
    /// Alpha representation.
    pub alpha_mode: AlphaMode,
}

impl<P: Pixel> TypedImage<P> {
    /// Creates a new typed image, validating that `pixels` length matches dimensions.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<P>,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
    ) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidDimensions { width, height });
        }
        let expected = width as usize * height as usize;
        if pixels.len() != expected {
            return Err(Error::invalid_param(format!(
                "pixel count {} does not match dimensions {}x{} (expected {})",
                pixels.len(),
                width,
                height,
                expected
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
            color_space,
            alpha_mode,
        })
    }

    /// Creates an image with uninitialized pixels (filled with `P::default()`).
    /// Useful for temporary buffers.
    pub fn new_with_default(
        width: u32,
        height: u32,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
    ) -> Self
    where
        P: Default,
    {
        let count = width as usize * height as usize;
        let pixels = (0..count).map(|_| P::default()).collect();
        Self {
            width,
            height,
            pixels,
            color_space,
            alpha_mode,
        }
    }

    /// Returns the number of pixels (`width * height`).
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Returns a reference to the pixel buffer.
    pub fn pixels(&self) -> &[P] {
        &self.pixels
    }

    /// Returns a mutable reference to the pixel buffer.
    pub fn pixels_mut(&mut self) -> &mut [P] {
        &mut self.pixels
    }

    /// Consumes the image and returns the pixel vector.
    pub fn into_pixels(self) -> Vec<P> {
        self.pixels
    }

    /// Returns a raw byte slice of the pixel data (via `bytemuck`).
    ///
    /// # Safety
    /// This is safe only if `P` is `Pod` (which all pixel types in this crate are).
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

    /// Returns the size in bytes of the pixel buffer.
    pub fn byte_len(&self) -> usize {
        self.pixels.len() * std::mem::size_of::<P>()
    }

    /// Returns `true` if the image has an alpha channel (according to pixel type).
    pub fn has_alpha(&self) -> bool {
        P::HAS_ALPHA
    }

    /// Returns a reference to the pixel at `(x, y)`.
    ///
    /// # Panics
    /// If `x >= width` or `y >= height`.
    pub fn pixel(&self, x: u32, y: u32) -> &P {
        let idx = y as usize * self.width as usize + x as usize;
        &self.pixels[idx]
    }

    /// Returns a mutable reference to the pixel at `(x, y)`.
    ///
    /// # Panics
    /// If `x >= width` or `y >= height`.
    pub fn pixel_mut(&mut self, x: u32, y: u32) -> &mut P {
        let idx = y as usize * self.width as usize + x as usize;
        &mut self.pixels[idx]
    }

    /// Iterates over pixels row‑by‑row.
    pub fn rows(&self) -> impl Iterator<Item = &[P]> {
        self.pixels.chunks_exact(self.width as usize)
    }

    /// Iterates mutably over pixels row‑by‑row.
    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut [P]> {
        let width = self.width as usize;
        self.pixels.chunks_exact_mut(width)
    }
}

