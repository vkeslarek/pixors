//! Abstract image metadata.
//!
//! Provides enums and types to describe image channels, alpha modes, and sample layouts.

use std::collections::HashMap;

#[derive(Default, Debug, Clone)]
pub struct ImageMetadata {
    pub source_format: Option<String>,
    pub source_path: Option<std::path::PathBuf>,
    pub dpi: Option<(f32, f32)>,
    pub text: HashMap<String, String>,
    pub raw_icc: Option<Vec<u8>>,
}

pub struct ImageInfo {
    pub layer_count: usize,
    pub metadata: ImageMetadata,
}

// --- From alpha.rs ---
/// Alpha representation (straight, premultiplied, or absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    /// Straight (unassociated) alpha: color channels are independent of alpha.
    /// The displayed color is `(r, g, b) * α` when compositing.
    Straight,

    /// Premultiplied (associated) alpha: color channels are already multiplied by alpha.
    /// The stored value is `(r*α, g*α, b*α, α)`.
    Premultiplied,

    /// No alpha channel; equivalent to α = 1.0 everywhere.
    Opaque,
}

impl AlphaMode {
    /// Returns `true` if the mode has an alpha channel (Straight or Premultiplied).
    pub fn has_alpha(self) -> bool {
        matches!(self, AlphaMode::Straight | AlphaMode::Premultiplied)
    }

    /// Returns `true` if alpha is straight (unassociated).
    pub fn is_straight(self) -> bool {
        matches!(self, AlphaMode::Straight)
    }

    /// Returns `true` if alpha is premultiplied.
    pub fn is_premultiplied(self) -> bool {
        matches!(self, AlphaMode::Premultiplied)
    }

    /// Returns `true` if alpha is opaque (no alpha channel).
    pub fn is_opaque(self) -> bool {
        matches!(self, AlphaMode::Opaque)
    }
}
// --- From channel.rs ---
/// Kind of a single channel (named).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    /// Red channel.
    R,
    /// Green channel.
    G,
    /// Blue channel.
    B,
    /// Alpha channel.
    A,
    /// Grayscale luminance.
    Gray,
    /// Luma (Y' in YUV).
    Y,
    /// Chrominance U (Cb).
    U,
    /// Chrominance V (Cr).
    V,
    /// Cyan (CMYK).
    Cyan,
    /// Magenta (CMYK).
    Magenta,
    /// Yellow (CMYK).
    Yellow,
    /// Black (CMYK).
    Black,
    /// Custom channel with numeric identifier.
    Custom(u16),
}

/// Arrangement of channels within an image.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChannelLayoutKind {
    /// 1 channel: Gray.
    Gray,
    /// 2 channels: Gray, Alpha.
    GrayAlpha,
    /// 3 channels: Red, Green, Blue.
    Rgb,
    /// 4 channels: Red, Green, Blue, Alpha.
    Rgba,
    /// 3 channels: Y, U, V (YUV).
    Yuv,
    /// 4 channels: Y, U, V, Alpha.
    YuvA,
    /// 4 channels: Cyan, Magenta, Yellow, Black.
    Cmyk,
    /// Arbitrary channel set (EXR‑style).
    Custom(Vec<ChannelKind>),
}

impl ChannelLayoutKind {
    /// Number of channels in this layout.
    pub fn channel_count(&self) -> usize {
        match self {
            ChannelLayoutKind::Gray => 1,
            ChannelLayoutKind::GrayAlpha => 2,
            ChannelLayoutKind::Rgb => 3,
            ChannelLayoutKind::Rgba => 4,
            ChannelLayoutKind::Yuv => 3,
            ChannelLayoutKind::YuvA => 4,
            ChannelLayoutKind::Cmyk => 4,
            ChannelLayoutKind::Custom(v) => v.len(),
        }
    }

    /// Returns `true` if the layout contains an alpha channel.
    pub fn has_alpha(&self) -> bool {
        match self {
            ChannelLayoutKind::GrayAlpha | ChannelLayoutKind::Rgba | ChannelLayoutKind::YuvA => {
                true
            }
            ChannelLayoutKind::Custom(v) => v.iter().any(|k| matches!(k, ChannelKind::A)),
            _ => false,
        }
    }

    /// Returns `true` if the layout is grayscale (single channel, not YUV).
    pub fn is_grayscale(&self) -> bool {
        matches!(self, ChannelLayoutKind::Gray | ChannelLayoutKind::GrayAlpha)
    }

    /// Returns `true` if the layout is RGB(A).
    pub fn is_rgb(&self) -> bool {
        matches!(self, ChannelLayoutKind::Rgb | ChannelLayoutKind::Rgba)
    }

    /// Iterator over the channel kinds in layout order.
    pub fn iter(&self) -> Box<dyn Iterator<Item = ChannelKind> + '_> {
        use ChannelKind::*;

        match self {
            ChannelLayoutKind::Gray => Box::new([Gray].into_iter()),
            ChannelLayoutKind::GrayAlpha => Box::new([Gray, A].into_iter()),
            ChannelLayoutKind::Rgb => Box::new([R, G, B].into_iter()),
            ChannelLayoutKind::Rgba => Box::new([R, G, B, A].into_iter()),
            ChannelLayoutKind::Yuv => Box::new([Y, U, V].into_iter()),
            ChannelLayoutKind::YuvA => Box::new([Y, U, V, A].into_iter()),
            ChannelLayoutKind::Cmyk => Box::new([Cyan, Magenta, Yellow, Black].into_iter()),
            ChannelLayoutKind::Custom(v) => Box::new(v.iter().copied()),
        }
    }
}
// --- From sample.rs ---
/// Numeric type of a single sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleType {
    /// Unsigned 8‑bit integer.
    U8,
    /// Unsigned 16‑bit integer.
    U16,
    /// Unsigned 32‑bit integer (rare; not supported in Phase 1).
    U32,
    /// 16‑bit floating‑point (IEEE‑754 binary16).
    F16,
    /// 32‑bit floating‑point (IEEE‑754 binary32).
    F32,
}

impl SampleType {
    /// Size in bytes of one sample.
    pub fn size_bytes(self) -> usize {
        match self {
            SampleType::U8 => 1,
            SampleType::U16 => 2,
            SampleType::U32 => 4,
            SampleType::F16 => 2,
            SampleType::F32 => 4,
        }
    }

    /// Returns `true` if the sample type is integer.
    pub fn is_integer(self) -> bool {
        matches!(self, SampleType::U8 | SampleType::U16 | SampleType::U32)
    }

    /// Returns `true` if the sample type is floating‑point.
    pub fn is_float(self) -> bool {
        matches!(self, SampleType::F16 | SampleType::F32)
    }

    /// Maximum representable value for integer types (2^bits - 1).
    /// For float types returns `1.0`.
    pub fn max_value(self) -> f32 {
        match self {
            SampleType::U8 => 255.0,
            SampleType::U16 => 65535.0,
            SampleType::U32 => 4294967295.0,
            SampleType::F16 => 1.0,
            SampleType::F32 => 1.0,
        }
    }

    /// Scaling factor to convert integer sample to `[0, 1]` floating‑point.
    /// For float types returns `1.0`.
    pub fn scale_to_f32(self) -> f32 {
        match self {
            SampleType::U8 => 1.0 / 255.0,
            SampleType::U16 => 1.0 / 65535.0,
            SampleType::U32 => 1.0 / 4294967295.0,
            SampleType::F16 | SampleType::F32 => 1.0,
        }
    }

    /// Scaling factor to convert `[0, 1]` floating‑point to integer sample.
    /// For float types returns `1.0`.
    pub fn scale_from_f32(self) -> f32 {
        match self {
            SampleType::U8 => 255.0,
            SampleType::U16 => 65535.0,
            SampleType::U32 => 4294967295.0,
            SampleType::F16 | SampleType::F32 => 1.0,
        }
    }
}

/// Arrangement of samples within a pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleLayout {
    /// Channels interleaved per pixel (`[R G B A R G B A …]`).
    Interleaved,
    /// Channels planar (`[R R R …, G G G …, B B B …, A A A …]`).
    Planar,
}

impl SampleLayout {
    /// Returns `true` if interleaved.
    pub fn is_interleaved(self) -> bool {
        matches!(self, SampleLayout::Interleaved)
    }

    /// Returns `true` if planar.
    pub fn is_planar(self) -> bool {
        matches!(self, SampleLayout::Planar)
    }
}
