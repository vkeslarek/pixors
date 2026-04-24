//! Lazy typed image: zero-copy view over raw buffer with on-demand color conversion.

use crate::error::Error;
use crate::color::{ColorSpace, ColorConversion};
use crate::pixel::{Pixel, Rgba};
use half::f16;
use std::sync::Arc;
use std::marker::PhantomData;

/// Lazy typed image: zero-copy view over `ImageBuffer` with color conversion on read.
///
/// Generic over pixel type `P`. Stores `Arc<ImageBuffer>` + `ColorConversion`.
/// No pixel data stored — conversion happens on-demand when pixels are read.
#[derive(Clone)]
pub struct TypedImage<P: Pixel> {
    /// Shared raw image data with layout descriptor.
    source: Arc<super::ImageBuffer>,
    /// Color conversion from source color space to target (depends on P).
    conv: ColorConversion,
    _phantom: PhantomData<P>,
}

impl<P: Pixel> TypedImage<P> {
    /// Image width in pixels.
    pub fn width(&self) -> u32 {
        self.source.desc.width
    }

    /// Image height in pixels.
    pub fn height(&self) -> u32 {
        self.source.desc.height
    }

    /// Total pixel count.
    pub fn pixel_count(&self) -> usize {
        self.width() as usize * self.height() as usize
    }

    /// Source color space (before conversion).
    pub fn source_color_space(&self) -> ColorSpace {
        self.source.desc.color_space
    }

    /// Alpha mode of the source data.
    pub fn alpha_mode(&self) -> super::AlphaMode {
        self.source.desc.alpha_mode
    }

    /// Return raw source buffer (for low-level access, e.g. direct disk I/O).
    pub fn source(&self) -> &Arc<super::ImageBuffer> {
        &self.source
    }
}

/// Specialization for Rgba<f16> ACEScg premultiplied (canonical working format).
impl TypedImage<Rgba<f16>> {
    /// Create a lazy view over raw image data with color conversion to ACEScg.
    /// Zero-copy: only wraps the `Arc` and builds the converter.
    pub fn from_raw(source: Arc<super::ImageBuffer>) -> Result<Self, Error> {
        let conv = source.desc.color_space.converter_to(ColorSpace::ACES_CG)?;
        Ok(Self { source, conv, _phantom: PhantomData })
    }

    /// Target color space (always ACEScg for this specialization).
    pub fn color_space(&self) -> ColorSpace {
        ColorSpace::ACES_CG
    }

    /// Read a rectangular region of pixels, converting on-demand to Rgba<f16> ACEScg premul.
    pub fn read_region(&self, x: u32, y: u32, w: u32, h: u32) -> Vec<Rgba<f16>> {
        let mut out = Vec::with_capacity((w * h) as usize);

        for py in y..y + h {
            for px in x..x + w {
                let r = self.source.read_sample(0, px, py);
                let g = self.source.read_sample(1, px, py);
                let b = self.source.read_sample(2, px, py);

                let a = if self.source.desc.planes.len() >= 4 {
                    self.source.read_sample(3, px, py)
                } else {
                    1.0
                };

                let linear = self.conv.decode_to_linear([r, g, b]);
                out.push(pack_rgba_premul(linear, a));
            }
        }

        out
    }

    /// Iterate over rows lazily, each row converted on-demand to Rgba<f16>.
    pub fn row_iter(&self) -> impl Iterator<Item = Vec<Rgba<f16>>> + '_ {
        (0..self.height()).map(move |y| self.read_region(0, y, self.width(), 1))
    }

    /// Read a single pixel at (x, y), converting on-demand.
    pub fn read_pixel(&self, x: u32, y: u32) -> Rgba<f16> {
        self.read_region(x, y, 1, 1)[0]
    }
}

/// Pack linear RGB + alpha into premultiplied Rgba<f16>.
#[inline(always)]
fn pack_rgba_premul(rgb: [f32; 3], a: f32) -> Rgba<f16> {
    Rgba {
        r: f16::from_f32(rgb[0] * a),
        g: f16::from_f32(rgb[1] * a),
        b: f16::from_f32(rgb[2] * a),
        a: f16::from_f32(a),
    }
}
