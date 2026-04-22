//! Raw image: runtime‑resolved metadata + raw bytes.

use crate::error::Error;
use crate::color::ColorSpace;
use super::{AlphaMode, ChannelLayoutKind, SampleType, SampleLayout};

/// Raw (runtime‑resolved) image representation.
///
/// This is what a file loader produces. The raw bytes are uninterpreted;
/// metadata describes how to decode them.
#[derive(Debug, Clone)]
pub struct RawImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,

    /// Sample numeric type.
    pub sample_type: SampleType,
    /// Channel arrangement.
    pub channel_layout: ChannelLayoutKind,
    /// Interleaved or planar storage.
    pub sample_layout: SampleLayout,

    /// Color space of the stored samples.
    pub color_space: ColorSpace,
    /// Alpha representation.
    pub alpha_mode: AlphaMode,

    /// Raw sample data.
    pub data: Vec<u8>,
}

impl RawImage {
    /// Creates a new raw image, validating that `data` length matches the expected size.
    pub fn new(
        width: u32,
        height: u32,
        sample_type: SampleType,
        channel_layout: ChannelLayoutKind,
        sample_layout: SampleLayout,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        data: Vec<u8>,
    ) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidDimensions { width, height });
        }
        let expected = Self::expected_data_len(width, height, sample_type, &channel_layout, sample_layout);
        if data.len() != expected {
            return Err(Error::invalid_param(format!(
                "data length {} does not match expected {} ({}x{}, {} channels, {} bytes per sample)",
                data.len(),
                expected,
                width,
                height,
                channel_layout.channel_count(),
                sample_type.size_bytes()
            )));
        }
        Ok(Self {
            width,
            height,
            sample_type,
            channel_layout,
            sample_layout,
            color_space,
            alpha_mode,
            data,
        })
    }

    /// Computes the expected byte length for the given parameters.
    pub fn expected_data_len(
        width: u32,
        height: u32,
        sample_type: SampleType,
        channel_layout: &ChannelLayoutKind,
        sample_layout: SampleLayout,
    ) -> usize {
        let pixels = width as usize * height as usize;
        let channels = channel_layout.channel_count();
        let bytes_per_sample = sample_type.size_bytes();

        match sample_layout {
            SampleLayout::Interleaved => pixels * channels * bytes_per_sample,
            SampleLayout::Planar => channels * pixels * bytes_per_sample,
        }
    }

    /// Returns the number of pixels (`width * height`).
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Returns the number of channels.
    pub fn channel_count(&self) -> usize {
        self.channel_layout.channel_count()
    }

    /// Returns `true` if the image has an alpha channel.
    pub fn has_alpha(&self) -> bool {
        self.channel_layout.has_alpha()
    }

    /// Returns the total number of samples (`pixel_count() * channel_count()`).
    pub fn sample_count(&self) -> usize {
        self.pixel_count() * self.channel_count()
    }

    /// Returns the size in bytes of a single pixel (all channels interleaved).
    /// For planar layouts this is the size of one pixel in the interleaved equivalent.
    pub fn pixel_bytes(&self) -> usize {
        self.channel_count() * self.sample_type.size_bytes()
    }

    /// Provides a raw slice to the sample data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consumes the raw image and returns its raw data.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}