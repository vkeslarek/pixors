use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::common::image::{Dpi, ImageDescriptor, Orientation, PageInfo};
use crate::common::color::space::ColorSpace;
use crate::common::pixel::{AlphaPolicy, PixelFormat};
use crate::error::Error;
use crate::graph::item::Item;

pub trait ImageDecoder: Send + Sync {
    fn probe(&self, path: &Path) -> Result<bool, Error>;
    fn decode(&self, path: &Path) -> Result<ImageDescriptor, Error>;
    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>, Error>;
}

pub trait PageStream: Send {
    fn page_info(&self) -> &PageInfo;
    fn drain(&mut self, max_items: usize) -> Result<Vec<Item>, Error>;
}

// ── Encoder interface ──────────────────────────────────────────────────────────

pub trait ImageEncoder: Send + Sync {
    fn probe(&self, path: &Path) -> bool;
    fn encode(
        &self,
        path: &Path,
        data: &[u8],
        desc: &EncoderDescriptor,
        config: &EncoderConfig,
    ) -> Result<(), Error>;
}

pub struct EncoderDescriptor {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub color_space: ColorSpace,
    pub alpha_policy: AlphaPolicy,
    pub dpi: Option<Dpi>,
    pub icc_profile: Option<Vec<u8>>,
    pub metadata: Vec<crate::common::image::exif::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum EncoderConfig {
    Png(PngExportConfig),
    Tiff(TiffExportConfig),
}

// ── PNG export config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngExportConfig {
    pub bit_depth: PngBitDepth,
    pub color_type: PngColorType,
    pub compression: PngCompression,
    pub filter: PngFilter,
    pub interlace: PngInterlace,
    pub embed_dpi: bool,
    pub embed_icc: bool,
    pub srgb_intent: Option<PngSrgbIntent>,
    pub gamma: Option<f64>,
    pub text_chunks: Vec<PngTextChunk>,
    pub animation: Option<PngAnimationConfig>,
}

impl Default for PngExportConfig {
    fn default() -> Self {
        Self {
            bit_depth: PngBitDepth::Eight,
            color_type: PngColorType::Rgba,
            compression: PngCompression::Default,
            filter: PngFilter::Adaptive,
            interlace: PngInterlace::None,
            embed_dpi: true,
            embed_icc: true,
            srgb_intent: None,
            gamma: None,
            text_chunks: vec![],
            animation: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngBitDepth {
    One,
    Two,
    Four,
    Eight,
    Sixteen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngColorType {
    Grayscale,
    GrayscaleAlpha,
    Rgb,
    Rgba,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngCompression {
    None,
    Fast,
    Default,
    Best,
    Level(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngFilter {
    None,
    Sub,
    Up,
    Average,
    Paeth,
    Adaptive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngInterlace {
    None,
    Adam7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngSrgbIntent {
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngTextChunk {
    pub keyword: String,
    pub text: String,
    pub encoding: PngTextEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngTextEncoding {
    Text,
    Ztxt,
    Itxt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngAnimationConfig {
    pub num_plays: u32,
    pub frames: Vec<PngFrameConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngFrameConfig {
    pub delay_numerator: u16,
    pub delay_denominator: u16,
    pub dispose_op: PngDisposeOp,
    pub blend_op: PngBlendOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngDisposeOp { None, Background, Previous }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngBlendOp { Source, Over }

// ── TIFF export config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiffExportConfig {
    pub bit_depth: TiffBitDepth,
    pub color_type: TiffColorType,
    pub compression: TiffCompression,
    pub layout: TiffLayout,
    pub tiff_variant: TiffVariant,
    pub byte_order: TiffByteOrder,
    pub embed_dpi: bool,
    pub embed_icc: bool,
    pub orientation: Orientation,
    pub embed_exif: bool,
    pub tags: TiffMetaTags,
    pub multipage: bool,
}

impl Default for TiffExportConfig {
    fn default() -> Self {
        Self {
            bit_depth: TiffBitDepth::Eight,
            color_type: TiffColorType::Rgb,
            compression: TiffCompression::Lzw {
                predictor: TiffPredictor::Horizontal,
            },
            layout: TiffLayout::Strip { rows_per_strip: 8 },
            tiff_variant: TiffVariant::Classic,
            byte_order: TiffByteOrder::LittleEndian,
            embed_dpi: true,
            embed_icc: true,
            orientation: Orientation::Identity,
            embed_exif: false,
            tags: TiffMetaTags::default(),
            multipage: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffBitDepth {
    Eight,
    Sixteen,
    ThirtyTwo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffColorType {
    Grayscale,
    GrayscaleAlpha,
    Rgb,
    Rgba,
    Cmyk,
    CmykAlpha,
    CieLab,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum TiffCompression {
    None,
    PackBits,
    Lzw { predictor: TiffPredictor },
    Deflate { level: u8, predictor: TiffPredictor },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffPredictor {
    None,
    Horizontal,
    FloatingPoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TiffLayout {
    Strip { rows_per_strip: u32 },
    Tile { tile_width: u32, tile_height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffVariant {
    Classic,
    BigTiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffByteOrder {
    LittleEndian,
    BigEndian,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TiffMetaTags {
    pub make: Option<String>,
    pub model: Option<String>,
    pub software: Option<String>,
    pub date_time: Option<String>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub image_description: Option<String>,
}

// ── Display impls (for UI pick lists) ──────────────────────────────────────────

impl std::fmt::Display for PngColorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngColorType::Grayscale => write!(f, "Grayscale"),
            PngColorType::GrayscaleAlpha => write!(f, "Grayscale + Alpha"),
            PngColorType::Rgb => write!(f, "RGB"),
            PngColorType::Rgba => write!(f, "RGBA"),
        }
    }
}

impl std::fmt::Display for PngBitDepth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngBitDepth::One => write!(f, "1-bit"),
            PngBitDepth::Two => write!(f, "2-bit"),
            PngBitDepth::Four => write!(f, "4-bit"),
            PngBitDepth::Eight => write!(f, "8-bit"),
            PngBitDepth::Sixteen => write!(f, "16-bit"),
        }
    }
}

impl std::fmt::Display for PngCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngCompression::None => write!(f, "None"),
            PngCompression::Fast => write!(f, "Fast"),
            PngCompression::Default => write!(f, "Default"),
            PngCompression::Best => write!(f, "Best"),
            PngCompression::Level(l) => write!(f, "Level {}", l),
        }
    }
}

impl std::fmt::Display for PngFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngFilter::None => write!(f, "None"),
            PngFilter::Sub => write!(f, "Sub"),
            PngFilter::Up => write!(f, "Up"),
            PngFilter::Average => write!(f, "Average"),
            PngFilter::Paeth => write!(f, "Paeth"),
            PngFilter::Adaptive => write!(f, "Adaptive"),
        }
    }
}

impl std::fmt::Display for TiffColorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiffColorType::Grayscale => write!(f, "Grayscale"),
            TiffColorType::GrayscaleAlpha => write!(f, "Grayscale + Alpha"),
            TiffColorType::Rgb => write!(f, "RGB"),
            TiffColorType::Rgba => write!(f, "RGBA"),
            TiffColorType::Cmyk => write!(f, "CMYK"),
            TiffColorType::CmykAlpha => write!(f, "CMYK + Alpha"),
            TiffColorType::CieLab => write!(f, "CIE Lab"),
        }
    }
}

impl std::fmt::Display for TiffBitDepth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiffBitDepth::Eight => write!(f, "8-bit"),
            TiffBitDepth::Sixteen => write!(f, "16-bit"),
            TiffBitDepth::ThirtyTwo => write!(f, "32-bit float"),
        }
    }
}

impl std::fmt::Display for TiffCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiffCompression::None => write!(f, "None"),
            TiffCompression::PackBits => write!(f, "PackBits"),
            TiffCompression::Lzw { .. } => write!(f, "LZW"),
            TiffCompression::Deflate { .. } => write!(f, "Deflate"),
        }
    }
}

impl std::fmt::Display for TiffLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiffLayout::Strip { rows_per_strip } => write!(f, "Strip ({rows_per_strip} rows)"),
            TiffLayout::Tile {
                tile_width,
                tile_height,
            } => write!(f, "Tile ({tile_width}×{tile_height})"),
        }
    }
}

impl std::fmt::Display for TiffPredictor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiffPredictor::None => write!(f, "None"),
            TiffPredictor::Horizontal => write!(f, "Horizontal"),
            TiffPredictor::FloatingPoint => write!(f, "Floating Point"),
        }
    }
}

impl std::fmt::Display for TiffByteOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiffByteOrder::LittleEndian => write!(f, "Little Endian"),
            TiffByteOrder::BigEndian => write!(f, "Big Endian"),
        }
    }
}

impl std::fmt::Display for PngInterlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngInterlace::None => write!(f, "None"),
            PngInterlace::Adam7 => write!(f, "Adam7"),
        }
    }
}
