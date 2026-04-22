//! Channel kinds and layouts (abstract image metadata).

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
            ChannelLayoutKind::GrayAlpha | ChannelLayoutKind::Rgba | ChannelLayoutKind::YuvA => true,
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