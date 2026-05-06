//! Stride-based buffer descriptor system for flexible image data layouts.

use crate::model::color::space::ColorSpace;
use crate::model::image::meta::AlphaMode;
use crate::model::pixel::PixelFormat;
use bytemuck::Pod;
use half::f16;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone)]
pub struct PlaneDescriptor {
    pub offset: usize,
    pub stride: usize,
    pub row_stride: usize,
    pub row_length: u32,
    pub encoding: PixelFormat,
    pub endian: Endian,
}

impl PlaneDescriptor {
    pub fn sample_offset(&self, x: u32, y: u32) -> usize {
        self.offset + (y as usize) * self.row_stride + (x as usize) * self.stride
    }

    pub fn read_sample(&self, data: &[u8], x: u32, y: u32) -> f32 {
        let off = self.sample_offset(x, y);
        match self.encoding {
            PixelFormat::Gray8 | PixelFormat::GrayA8 | PixelFormat::Rgb8
            | PixelFormat::Rgba8 | PixelFormat::Argb32 => data[off] as f32 / 255.0,
            pf @ (PixelFormat::Gray16 | PixelFormat::GrayA16 | PixelFormat::Rgb16
            | PixelFormat::Rgba16) => {
                let raw = if self.endian == Endian::Little {
                    u16::from_le_bytes([data[off], data[off + 1]])
                } else {
                    u16::from_be_bytes([data[off], data[off + 1]])
                };
                raw as f32 / 65535.0
            }
            pf @ (PixelFormat::GrayF16 | PixelFormat::GrayAF16 | PixelFormat::RgbF16
            | PixelFormat::RgbaF16) => {
                let bits = if self.endian == Endian::Little {
                    u16::from_le_bytes([data[off], data[off + 1]])
                } else {
                    u16::from_be_bytes([data[off], data[off + 1]])
                };
                f16::from_bits(bits).to_f32()
            }
            PixelFormat::GrayF32 | PixelFormat::GrayAF32 | PixelFormat::RgbF32
            | PixelFormat::RgbaF32 => {
                let arr = [data[off], data[off + 1], data[off + 2], data[off + 3]];
                if self.endian == Endian::Little {
                    f32::from_le_bytes(arr)
                } else {
                    f32::from_be_bytes(arr)
                }
            }
        }
    }

    pub fn row_range(&self, row_start: u32, row_end: u32) -> Range<usize> {
        let start = self.offset + (row_start as usize) * self.row_stride;
        let end = self.offset + (row_end as usize) * self.row_stride;
        start..end
    }

    pub fn planar_row<'a, T: Pod>(&self, data: &'a [u8], y: u32) -> Option<&'a [T]> {
        if self.stride != std::mem::size_of::<T>() {
            return None;
        }
        let byte_start = self.offset + y as usize * self.row_stride;
        let byte_end = byte_start + self.row_length as usize * self.stride;
        let bytes = data.get(byte_start..byte_end)?;
        Some(bytemuck::cast_slice(bytes))
    }

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

#[derive(Debug, Clone)]
pub struct BufferDescriptor {
    pub width: u32,
    pub height: u32,
    pub planes: Vec<PlaneDescriptor>,
    pub color_space: ColorSpace,
    pub alpha_mode: AlphaMode,
}

impl BufferDescriptor {
    pub fn total_bytes(&self) -> usize {
        self.planes
            .iter()
            .map(|p| p.offset + self.height as usize * p.row_stride)
            .max()
            .unwrap_or(0)
    }

    pub fn is_planar(&self) -> bool {
        self.planes
            .iter()
            .all(|p| p.stride == p.encoding.sample_bytes())
    }

    pub fn is_interleaved_packed(&self, channels: u8) -> bool {
        if self.planes.len() != channels as usize {
            return false;
        }
        let byte_size = self.planes[0].encoding.sample_bytes();
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

    fn interleaved(
        width: u32,
        height: u32,
        encoding: PixelFormat,
        endian: Endian,
        offsets: &[usize],
        stride: usize,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
    ) -> Self {
        let row_stride = width as usize * stride;
        let planes: Vec<_> = offsets
            .iter()
            .map(|&offset| PlaneDescriptor {
                offset,
                stride,
                row_stride,
                row_length: width,
                encoding,
                endian,
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

    const LE: Endian = Endian::Little;
    const BE: Endian = Endian::Big;

    pub fn rgba8_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray8, Self::LE, &[0, 1, 2, 3], 4, cs, a)
    }
    pub fn rgb8_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray8, Self::LE, &[0, 1, 2], 3, cs, a)
    }
    pub fn gray8_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray8, Self::LE, &[0], 1, cs, a)
    }
    pub fn gray_alpha8_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray8, Self::LE, &[0, 1], 2, cs, a)
    }

    pub fn rgba16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::LE, &[0, 2, 4, 6], 8, cs, a)
    }
    pub fn rgb16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::LE, &[0, 2, 4], 6, cs, a)
    }
    pub fn gray16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::LE, &[0], 2, cs, a)
    }
    pub fn gray_alpha16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::LE, &[0, 2], 4, cs, a)
    }

    pub fn rgba16be_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::BE, &[0, 2, 4, 6], 8, cs, a)
    }
    pub fn rgb16be_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::BE, &[0, 2, 4], 6, cs, a)
    }
    pub fn gray16be_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::BE, &[0], 2, cs, a)
    }
    pub fn gray_alpha16be_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::Gray16, Self::BE, &[0, 2], 4, cs, a)
    }

    pub fn rgba_f16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF16, Self::LE, &[0, 2, 4, 6], 8, cs, a)
    }
    pub fn rgb_f16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF16, Self::LE, &[0, 2, 4], 6, cs, a)
    }
    pub fn gray_f16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF16, Self::LE, &[0], 2, cs, a)
    }
    pub fn gray_alpha_f16_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF16, Self::LE, &[0, 2], 4, cs, a)
    }

    pub fn rgba_f32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0, 4, 8, 12], 16, cs, a)
    }
    pub fn rgb_f32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0, 4, 8], 12, cs, a)
    }
    pub fn gray_f32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0], 4, cs, a)
    }
    pub fn gray_alpha_f32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0, 4], 8, cs, a)
    }

    pub fn rgba32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0, 4, 8, 12], 16, cs, a)
    }
    pub fn rgb32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0, 4, 8], 12, cs, a)
    }
    pub fn gray32_interleaved(w: u32, h: u32, cs: ColorSpace, a: AlphaMode) -> Self {
        Self::interleaved(w, h, PixelFormat::GrayF32, Self::LE, &[0], 4, cs, a)
    }
}

pub struct ImageBuffer {
    pub desc: BufferDescriptor,
    pub data: Vec<u8>,
}
