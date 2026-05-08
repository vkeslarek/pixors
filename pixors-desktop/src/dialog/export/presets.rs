use pixors_executor::common::image::codec::{
    PngBitDepth, PngColorType, PngCompression, PngFilter, PngInterlace, TiffBitDepth,
    TiffByteOrder, TiffColorType, TiffCompression, TiffLayout, TiffPredictor,
};

// ── PNG compression preset ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PngCompressionPreset {
    None,
    Fast,
    Default,
    Best,
    Custom,
}

impl std::fmt::Display for PngCompressionPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::None => "None",
            Self::Fast => "Fast",
            Self::Default => "Default",
            Self::Best => "Best",
            Self::Custom => "Custom level",
        };
        write!(f, "{s}")
    }
}

pub fn png_preset(c: PngCompression) -> PngCompressionPreset {
    match c {
        PngCompression::None => PngCompressionPreset::None,
        PngCompression::Fast => PngCompressionPreset::Fast,
        PngCompression::Default => PngCompressionPreset::Default,
        PngCompression::Best => PngCompressionPreset::Best,
        PngCompression::Level(_) => PngCompressionPreset::Custom,
    }
}

// ── TIFF compression kind ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TiffCompressionKind {
    None,
    PackBits,
    Lzw,
    Deflate,
}

impl std::fmt::Display for TiffCompressionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::None => "None",
            Self::PackBits => "PackBits",
            Self::Lzw => "LZW",
            Self::Deflate => "Deflate",
        };
        write!(f, "{s}")
    }
}

pub fn tiff_compression_kind(c: &TiffCompression) -> TiffCompressionKind {
    match c {
        TiffCompression::None => TiffCompressionKind::None,
        TiffCompression::PackBits => TiffCompressionKind::PackBits,
        TiffCompression::Lzw { .. } => TiffCompressionKind::Lzw,
        TiffCompression::Deflate { .. } => TiffCompressionKind::Deflate,
    }
}

pub fn current_tiff_predictor(c: &TiffCompression) -> TiffPredictor {
    match c {
        TiffCompression::Lzw { predictor } | TiffCompression::Deflate { predictor, .. } => {
            *predictor
        }
        _ => TiffPredictor::Horizontal,
    }
}

pub fn current_deflate_level(c: &TiffCompression) -> u8 {
    match c {
        TiffCompression::Deflate { level, .. } => *level,
        _ => 6,
    }
}

// ── TIFF layout kind ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TiffLayoutKind {
    Strip,
    Tile,
}

impl std::fmt::Display for TiffLayoutKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Strip => write!(f, "Strip"),
            Self::Tile => write!(f, "Tile"),
        }
    }
}

pub fn tiff_layout_kind(l: &TiffLayout) -> TiffLayoutKind {
    match l {
        TiffLayout::Strip { .. } => TiffLayoutKind::Strip,
        TiffLayout::Tile { .. } => TiffLayoutKind::Tile,
    }
}

// ── Static option lists ───────────────────────────────────────────────────────

pub static PNG_COMPRESSION_PRESETS: &[PngCompressionPreset] = &[
    PngCompressionPreset::None,
    PngCompressionPreset::Fast,
    PngCompressionPreset::Default,
    PngCompressionPreset::Best,
    PngCompressionPreset::Custom,
];

pub static TIFF_COMPRESSION_KINDS: &[TiffCompressionKind] = &[
    TiffCompressionKind::None,
    TiffCompressionKind::PackBits,
    TiffCompressionKind::Lzw,
    TiffCompressionKind::Deflate,
];

pub static PNG_COLOR_TYPES: &[PngColorType] = &[
    PngColorType::Rgba,
    PngColorType::Rgb,
    PngColorType::Grayscale,
    PngColorType::GrayscaleAlpha,
];

pub static PNG_BIT_DEPTHS: &[PngBitDepth] = &[PngBitDepth::Eight, PngBitDepth::Sixteen];

pub static PNG_FILTERS: &[PngFilter] = &[
    PngFilter::Adaptive,
    PngFilter::None,
    PngFilter::Sub,
    PngFilter::Up,
    PngFilter::Average,
    PngFilter::Paeth,
];

pub static PNG_INTERLACES: &[PngInterlace] = &[PngInterlace::None, PngInterlace::Adam7];

pub static TIFF_COLOR_TYPES: &[TiffColorType] = &[
    TiffColorType::Rgb,
    TiffColorType::Rgba,
    TiffColorType::Grayscale,
    TiffColorType::GrayscaleAlpha,
    TiffColorType::Cmyk,
    TiffColorType::CmykAlpha,
    TiffColorType::CieLab,
];

pub static TIFF_BIT_DEPTHS: &[TiffBitDepth] = &[
    TiffBitDepth::Eight,
    TiffBitDepth::Sixteen,
    TiffBitDepth::ThirtyTwo,
];

pub static TIFF_PREDICTORS_INT: &[TiffPredictor] =
    &[TiffPredictor::None, TiffPredictor::Horizontal];

pub static TIFF_PREDICTORS_ALL: &[TiffPredictor] = &[
    TiffPredictor::None,
    TiffPredictor::Horizontal,
    TiffPredictor::FloatingPoint,
];

pub static TIFF_BYTE_ORDERS: &[TiffByteOrder] =
    &[TiffByteOrder::LittleEndian, TiffByteOrder::BigEndian];
