use half::f16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Straight,
    Premultiplied,
    Opaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    R,
    G,
    B,
    A,
    Gray,
    Y,
    U,
    V,
    Cyan,
    Magenta,
    Yellow,
    Black,
    Custom(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChannelLayoutKind {
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
    Yuv,
    YuvA,
    Cmyk,
    Custom(Vec<ChannelKind>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleType {
    U8,
    U16,
    U32,
    F16,
    F32,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleLayout {
    Interleaved,
    Planar,
}
