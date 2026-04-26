//! Stride-based buffer descriptor system for flexible image data layouts.
//!
//! Describes any image layout via per-channel descriptors (offset, stride, row_stride).
//! Enables interleaved, planar, padded, and mixed layouts without hardcoding assumptions.

use crate::color::{ColorSpace};
use bytemuck::Pod;
use half::f16;
use std::ops::Range;

// ---------------------------------------------------------------------------
// Sample Format
// ---------------------------------------------------------------------------

/// Normalized [0,1] sample format with explicit endianness.
///
/// All reads produce a normalized f32 value. Integer types are divided by
/// their maximum (255 for u8, 65535 for u16). Float types pass through
/// unclamped (HDR float buffers must not be clamped to [0,1]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    U8,
    U16Le,
    U16Be,
    U32Le,
    U32Be,
    F16Le,
    F16Be,
    F32Le,
    F32Be,
}

impl SampleFormat {
    pub fn byte_size(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16Le | Self::U16Be | Self::F16Le | Self::F16Be => 2,
            Self::U32Le | Self::U32Be | Self::F32Le | Self::F32Be => 4,
        }
    }

    /// Read one sample and return normalized f32 ([0,1] for integers,
    /// unclamped for floats).
    pub fn read_sample(self, data: &[u8], offset: usize) -> f32 {
        match self {
            Self::U8 => data[offset] as f32 / 255.0,
            Self::U16Le => {
                u16::from_le_bytes([data[offset], data[offset + 1]]) as f32 / 65535.0
            }
            Self::U16Be => {
                u16::from_be_bytes([data[offset], data[offset + 1]]) as f32 / 65535.0
            }
            Self::U32Le => {
                let val = u32::from_le_bytes([
                    data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
                ]);
                val as f32 / u32::MAX as f32
            }
            Self::U32Be => {
                let val = u32::from_be_bytes([
                    data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
                ]);
                val as f32 / u32::MAX as f32
            }
            Self::F16Le => {
                let bits = u16::from_le_bytes([data[offset], data[offset + 1]]);
                f16::from_bits(bits).to_f32()
            }
            Self::F16Be => {
                let bits = u16::from_be_bytes([data[offset], data[offset + 1]]);
                f16::from_bits(bits).to_f32()
            }
            Self::F32Le => {
                f32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ])
            }
            Self::F32Be => {
                f32::from_be_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ])
            }
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
    pub encoding: SampleFormat,
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

    // -----------------------------------------------------------------------
    // Typed-row fast paths (skip per-sample dispatch in SIMD hot loops)
    // -----------------------------------------------------------------------

    /// Planar layout fast path: returns the whole row as `&[T]` when
    /// `stride == size_of::<T>()`. Returns `None` if the layout is exotic
    /// (interleaved, misaligned, mismatched stride).
    pub fn planar_row<'a, T: Pod>(&self, data: &'a [u8], y: u32) -> Option<&'a [T]> {
        if self.stride != std::mem::size_of::<T>() {
            return None;
        }
        let byte_start = self.offset + y as usize * self.row_stride;
        let byte_end = byte_start + self.row_length as usize * self.stride;
        let bytes = data.get(byte_start..byte_end)?;
        Some(bytemuck::cast_slice(bytes))
    }

    /// Interleaved layout fast path: returns the whole row as `&[T]` where
    /// `stride == N * size_of::<T>()`. The caller indexes `chunks_exact(N)`.
    /// Returns `None` if the stride doesn't match the expected channel count.
    pub fn interleaved_row<'a, T: Pod, const N: usize>(
        &self,
        data: &'a [u8],
        y: u32,
    ) -> Option<&'a [T]> {
        if self.stride != N * std::mem::size_of::<T>() {
            return None;
        }
        let byte_start = self.offset + y as usize * self.row_stride;
        let byte_end = byte_start + self.row_length as usize * self.stride;
        let bytes = data.get(byte_start..byte_end)?;
        Some(bytemuck::cast_slice(bytes))
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

    /// Returns true when all planes have `stride == encoding.byte_size()` (planar).
    pub fn is_planar(&self) -> bool {
        self.planes.iter().all(|p| p.stride == p.encoding.byte_size())
    }

    /// Returns true when planes are interleaved-packed: same row_stride,
    /// consecutive offsets, N channels.
    pub fn is_interleaved_packed(&self, channels: u8) -> bool {
        if self.planes.len() != channels as usize {
            return false;
        }
        let byte_size = self.planes[0].encoding.byte_size();
        let expected_stride = channels as usize * byte_size;
        for (i, p) in self.planes.iter().enumerate() {
            if p.offset != i * byte_size {
                return false;
            }
            if p.stride != expected_stride {
                return false;
            }
            if p.row_stride != self.planes[0].row_stride {
                return false;
            }
        }
        true
    }

    /// Generic helper: interleaved buffer with given parameters.
    fn interleaved(
        width: u32,
        height: u32,
        format: SampleFormat,
        offsets: &[usize],
        stride: usize,
        color_space: ColorSpace,
        alpha_mode: crate::image::AlphaMode,
    ) -> Self {
        let row_stride = width as usize * stride;
        let planes: Vec<_> = offsets
            .iter()
            .map(|&offset| PlaneDesc {
                offset,
                stride,
                row_stride,
                row_length: width,
                encoding: format,
            })
            .collect();
        Self {
            width,
            height,
            planes,
            color_space,
            alpha_mode,
        }
    }

    pub fn rgba8_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U8, &[0, 1, 2, 3], 4, cs, a)
    }
    pub fn rgb8_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U8, &[0, 1, 2], 3, cs, a)
    }
    pub fn gray8_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U8, &[0], 1, cs, a)
    }
    pub fn gray_alpha8_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U8, &[0, 1], 2, cs, a)
    }
    #[cfg(target_endian = "little")]
    const U16_NATIVE: SampleFormat = SampleFormat::U16Le;
    #[cfg(target_endian = "big")]
    const U16_NATIVE: SampleFormat = SampleFormat::U16Be;

    pub fn rgba16_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U16_NATIVE, &[0, 2, 4, 6], 8, cs, a)
    }
    pub fn rgb16_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U16_NATIVE, &[0, 2, 4], 6, cs, a)
    }
    pub fn gray16_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U16_NATIVE, &[0], 2, cs, a)
    }

    pub fn gray_alpha16_interleaved(
        w: u32,
        h: u32,
        cs: ColorSpace,
        a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U16_NATIVE, &[0, 2], 4, cs, a)
    }

    // --- Explicit big-endian 16-bit (for PNG, which leaves data BE) ---

    pub fn rgba16be_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U16Be, &[0, 2, 4, 6], 8, cs, a)
    }
    pub fn rgb16be_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U16Be, &[0, 2, 4], 6, cs, a)
    }
    pub fn gray16be_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U16Be, &[0], 2, cs, a)
    }
    pub fn gray_alpha16be_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, SampleFormat::U16Be, &[0, 2], 4, cs, a)
    }

    // --- Host-native endian format constants ---

    #[cfg(target_endian = "little")]
    const F16_NATIVE: SampleFormat = SampleFormat::F16Le;
    #[cfg(target_endian = "big")]
    const F16_NATIVE: SampleFormat = SampleFormat::F16Be;

    #[cfg(target_endian = "little")]
    const F32_NATIVE: SampleFormat = SampleFormat::F32Le;
    #[cfg(target_endian = "big")]
    const F32_NATIVE: SampleFormat = SampleFormat::F32Be;

    #[cfg(target_endian = "little")]
    const U32_NATIVE: SampleFormat = SampleFormat::U32Le;
    #[cfg(target_endian = "big")]
    const U32_NATIVE: SampleFormat = SampleFormat::U32Be;

    // --- f16 family (host-native, TIFF crate output) ---

    pub fn rgba_f16_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F16_NATIVE, &[0, 2, 4, 6], 8, cs, a)
    }
    pub fn rgb_f16_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F16_NATIVE, &[0, 2, 4], 6, cs, a)
    }
    pub fn gray_f16_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F16_NATIVE, &[0], 2, cs, a)
    }
    pub fn gray_alpha_f16_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F16_NATIVE, &[0, 2], 4, cs, a)
    }

    // --- f32 family (host-native) ---

    pub fn rgba_f32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F32_NATIVE, &[0, 4, 8, 12], 16, cs, a)
    }
    pub fn rgb_f32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F32_NATIVE, &[0, 4, 8], 12, cs, a)
    }
    pub fn gray_f32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F32_NATIVE, &[0], 4, cs, a)
    }
    pub fn gray_alpha_f32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::F32_NATIVE, &[0, 4], 8, cs, a)
    }

    // --- u32 family (host-native) ---

    pub fn rgba32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U32_NATIVE, &[0, 4, 8, 12], 16, cs, a)
    }
    pub fn rgb32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U32_NATIVE, &[0, 4, 8], 12, cs, a)
    }
    pub fn gray32_interleaved(
        w: u32, h: u32, cs: ColorSpace, a: crate::image::AlphaMode,
    ) -> Self {
        Self::interleaved(w, h, Self::U32_NATIVE, &[0], 4, cs, a)
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
