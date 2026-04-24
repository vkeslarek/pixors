//! Stride-based buffer descriptor system for flexible image data layouts.
//!
//! Describes any image layout via per-channel descriptors (offset, stride, row_stride).
//! Enables interleaved, planar, padded, and mixed layouts without hardcoding assumptions.

use crate::color::{ColorSpace, ColorConversion};
use crate::error::Error;
use crate::pixel::Rgba;
use half::f16;
use std::ops::Range;

// ---------------------------------------------------------------------------
// Component Encoding
// ---------------------------------------------------------------------------

/// How a single component value is encoded in the buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComponentEncoding {
    /// Unsigned integer (u8, u16).
    UnsignedInt { bits: u8 },
    /// Unsigned normalized to [0.0, 1.0] (u8 → 0/255, u16 → 0/65535).
    UnsignedNormalized { bits: u8 },
    /// IEEE floating point (f16 or f32).
    Float { bits: u8 },
}

impl ComponentEncoding {
    /// Byte size for this encoding.
    pub fn byte_size(&self) -> usize {
        match self {
            Self::UnsignedInt { bits } | Self::UnsignedNormalized { bits } => (*bits as usize + 7) / 8,
            Self::Float { bits } => (*bits as usize + 7) / 8,
        }
    }

    /// Read one sample from buffer and return as normalized f32.
    pub fn read_sample(&self, data: &[u8], offset: usize) -> f32 {
        match self {
            Self::UnsignedInt { bits: 8 } => data[offset] as f32,
            Self::UnsignedInt { bits: 16 } => {
                let val = u16::from_be_bytes([data[offset], data[offset + 1]]);
                val as f32
            }
            Self::UnsignedNormalized { bits: 8 } => {
                let val = data[offset] as f32;
                val / 255.0
            }
            Self::UnsignedNormalized { bits: 16 } => {
                let val = u16::from_be_bytes([data[offset], data[offset + 1]]) as f32;
                val / 65535.0
            }
            Self::Float { bits: 16 } => {
                let bits = u16::from_be_bytes([data[offset], data[offset + 1]]);
                f16::from_bits(bits).to_f32()
            }
            Self::Float { bits: 32 } => {
                let val = f32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
                val.clamp(0.0, 1.0)
            }
            _ => 0.0, // unsupported
        }
    }
}

// ---------------------------------------------------------------------------
// Plane Descriptor
// ---------------------------------------------------------------------------

/// Describes one component plane in a buffer (one channel for planar,
/// or all channels for interleaved).
#[derive(Debug, Clone)]
pub struct PlaneDesc {
    /// Byte offset of the first sample from buffer start.
    pub offset: usize,
    /// Bytes between consecutive samples within a row (e.g., 4 for RGBA interleaved).
    pub stride: usize,
    /// Bytes from start of row N to start of row N+1 (includes padding).
    pub row_stride: usize,
    /// Number of pixels per row (not bytes).
    pub row_length: u32,
    /// How samples are encoded.
    pub encoding: ComponentEncoding,
}

impl PlaneDesc {
    /// Byte offset of the sample at (x, y) in the buffer.
    pub fn sample_offset(&self, x: u32, y: u32) -> usize {
        self.offset + (y as usize) * self.row_stride + (x as usize) * self.stride
    }

    /// Read one sample at (x, y) as normalized f32.
    pub fn read_sample(&self, data: &[u8], x: u32, y: u32) -> f32 {
        let offset = self.sample_offset(x, y);
        self.encoding.read_sample(data, offset)
    }

    /// Byte range covering rows [row_start, row_end).
    pub fn row_range(&self, row_start: u32, row_end: u32) -> Range<usize> {
        let start = self.offset + (row_start as usize) * self.row_stride;
        let end = self.offset + (row_end as usize) * self.row_stride;
        start..end
    }
}

// ---------------------------------------------------------------------------
// Buffer Descriptor
// ---------------------------------------------------------------------------

/// Full image buffer descriptor describing layout and color properties.
#[derive(Debug, Clone)]
pub struct BufferDesc {
    pub width: u32,
    pub height: u32,
    /// One PlaneDesc per channel (RGBA = 4, RGB = 3, etc.).
    pub planes: Vec<PlaneDesc>,
    pub color_space: ColorSpace,
    pub alpha_mode: crate::image::AlphaMode,
}

impl BufferDesc {
    /// Total bytes needed for the full buffer.
    pub fn total_bytes(&self) -> usize {
        self.planes
            .iter()
            .map(|p| p.offset + self.height as usize * p.row_stride)
            .max()
            .unwrap_or(0)
    }

    /// Factory: RGBA8 interleaved (common for PNG output).
    pub fn rgba8_interleaved(
        width: u32,
        height: u32,
        color_space: ColorSpace,
        alpha_mode: crate::image::AlphaMode,
    ) -> Self {
        let row_stride = width as usize * 4;
        let enc = ComponentEncoding::UnsignedNormalized { bits: 8 };
        Self {
            width,
            height,
            planes: vec![
                PlaneDesc { offset: 0, stride: 4, row_stride, row_length: width, encoding: enc.clone() },
                PlaneDesc { offset: 1, stride: 4, row_stride, row_length: width, encoding: enc.clone() },
                PlaneDesc { offset: 2, stride: 4, row_stride, row_length: width, encoding: enc.clone() },
                PlaneDesc { offset: 3, stride: 4, row_stride, row_length: width, encoding: enc },
            ],
            color_space,
            alpha_mode,
        }
    }

    /// Factory: RGB8 interleaved.
    pub fn rgb8_interleaved(
        width: u32,
        height: u32,
        color_space: ColorSpace,
        alpha_mode: crate::image::AlphaMode,
    ) -> Self {
        let row_stride = width as usize * 3;
        let enc = ComponentEncoding::UnsignedNormalized { bits: 8 };
        Self {
            width,
            height,
            planes: vec![
                PlaneDesc { offset: 0, stride: 3, row_stride, row_length: width, encoding: enc.clone() },
                PlaneDesc { offset: 1, stride: 3, row_stride, row_length: width, encoding: enc.clone() },
                PlaneDesc { offset: 2, stride: 3, row_stride, row_length: width, encoding: enc },
            ],
            color_space,
            alpha_mode,
        }
    }

    /// Factory: Grayscale (single channel) 8-bit interleaved.
    pub fn gray8_interleaved(
        width: u32,
        height: u32,
        color_space: ColorSpace,
        alpha_mode: crate::image::AlphaMode,
    ) -> Self {
        let row_stride = width as usize;
        let enc = ComponentEncoding::UnsignedNormalized { bits: 8 };
        Self {
            width,
            height,
            planes: vec![
                PlaneDesc { offset: 0, stride: 1, row_stride, row_length: width, encoding: enc },
            ],
            color_space,
            alpha_mode,
        }
    }

    /// Factory: Grayscale + Alpha (2 channels) 8-bit interleaved.
    pub fn gray_alpha8_interleaved(
        width: u32,
        height: u32,
        color_space: ColorSpace,
        alpha_mode: crate::image::AlphaMode,
    ) -> Self {
        let row_stride = width as usize * 2;
        let enc = ComponentEncoding::UnsignedNormalized { bits: 8 };
        Self {
            width,
            height,
            planes: vec![
                PlaneDesc { offset: 0, stride: 2, row_stride, row_length: width, encoding: enc.clone() },
                PlaneDesc { offset: 1, stride: 2, row_stride, row_length: width, encoding: enc },
            ],
            color_space,
            alpha_mode,
        }
    }
}

// ---------------------------------------------------------------------------
// Image Buffer
// ---------------------------------------------------------------------------

/// Owned buffer: descriptor + raw bytes.
pub struct ImageBuffer {
    pub desc: BufferDesc,
    pub data: Vec<u8>,
}

impl ImageBuffer {
    /// Allocate a buffer matching the descriptor.
    pub fn allocate(desc: BufferDesc) -> Self {
        let size = desc.total_bytes();
        Self {
            desc,
            data: vec![0u8; size],
        }
    }

    /// Write raw row data into the buffer at the given row index.
    /// Assumes all planes use the same row_stride (packed layout).
    pub fn write_row(&mut self, row: u32, src: &[u8]) {
        let row_stride = self.desc.planes[0].row_stride;
        let start = row as usize * row_stride;
        let end = start + src.len().min(row_stride);
        self.data[start..end].copy_from_slice(&src[..end - start]);
    }

    /// Read one sample at (plane_idx, x, y) as normalized f32.
    pub fn read_sample(&self, plane_idx: usize, x: u32, y: u32) -> f32 {
        if plane_idx >= self.desc.planes.len() {
            return 0.0;
        }
        self.desc.planes[plane_idx].read_sample(&self.data, x, y)
    }
}

// ---------------------------------------------------------------------------
// Band Buffer — streaming workhorse
// ---------------------------------------------------------------------------

/// Preallocated contiguous buffer for one horizontal tile row band.
/// Full width × tile_size rows. Reused across all bands in a stream.
pub struct BandBuffer {
    buffer: ImageBuffer,
    rows_filled: u32,
    pub tile_size: u32,
    pub band_y: u32, // absolute Y coordinate of the first row in this band
}

impl BandBuffer {
    /// Allocate a new band buffer for streaming.
    pub fn new(image_width: u32, tile_size: u32, color_space: ColorSpace, alpha_mode: crate::image::AlphaMode) -> Self {
        // Band descriptor: full width × tile_size height, RGBA8 interleaved
        let desc = BufferDesc::rgba8_interleaved(image_width, tile_size, color_space, alpha_mode);
        let buffer = ImageBuffer::allocate(desc);
        Self {
            buffer,
            rows_filled: 0,
            tile_size,
            band_y: 0,
        }
    }

    /// Write the next row of decoded pixels into the band buffer.
    pub fn push_row(&mut self, row_data: &[u8]) {
        self.buffer.write_row(self.rows_filled, row_data);
        self.rows_filled += 1;
    }

    /// Check if the band is full (rows_filled >= tile_size).
    pub fn is_full(&self) -> bool {
        self.rows_filled >= self.tile_size
    }

    /// Get the number of rows currently filled in this band.
    pub fn rows_filled(&self) -> u32 {
        self.rows_filled
    }

    /// Reset for the next band (reuse memory).
    pub fn reset(&mut self, new_band_y: u32) {
        self.rows_filled = 0;
        self.band_y = new_band_y;
    }

    /// Extract a tile region as Rgba<f16> (ACEScg premultiplied).
    /// tx = tile column start (in pixels)
    /// tile_width, tile_height = tile dimensions
    /// actual_height = actual rows in this band (may be < tile_size for last band)
    pub fn extract_tile_rgba_f16(
        &self,
        tx: u32,
        tile_width: u32,
        actual_height: u32,
        conv: &ColorConversion,
    ) -> Result<Vec<Rgba<f16>>, Error> {
        let capacity = (tile_width * actual_height) as usize;
        let mut out = Vec::with_capacity(capacity);

        for row in 0..actual_height {
            for col in 0..tile_width {
                let px = tx + col;
                if px >= self.buffer.desc.width {
                    break;
                }

                let r = self.buffer.read_sample(0, px, row);
                let g = self.buffer.read_sample(1, px, row);
                let b = self.buffer.read_sample(2, px, row);
                let a = self.buffer.read_sample(3, px, row);

                let linear_rgb = conv.decode_to_linear([r, g, b]);
                out.push(pack_rgba_premul(linear_rgb, a));
            }
        }

        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
