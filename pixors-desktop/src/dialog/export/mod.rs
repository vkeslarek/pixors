pub mod components;
pub mod png;
pub mod presets;
pub mod tiff;
pub mod view;

use iced::Element;
use pixors_executor::common::image::codec::{
    EncoderConfig, PngBitDepth, PngColorType, PngCompression, PngExportConfig, PngFilter,
    PngInterlace, TiffBitDepth, TiffByteOrder, TiffColorType, TiffCompression, TiffExportConfig,
    TiffLayout, TiffPredictor, TiffVariant,
};

use presets::{
    current_deflate_level, current_tiff_predictor, PngCompressionPreset, TiffCompressionKind,
    TiffLayoutKind,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    Png,
    Tiff,
}

impl ExportFormat {
    pub fn name(&self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Tiff => "TIFF",
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    FormatChanged(ExportFormat),
    // PNG
    PngColorType(PngColorType),
    PngBitDepth(PngBitDepth),
    PngCompressionPreset(PngCompressionPreset),
    PngDeflateLevel(f32),
    PngFilter(PngFilter),
    PngInterlace(PngInterlace),
    PngEmbedDpi(bool),
    PngEmbedIcc(bool),
    // TIFF
    TiffColorType(TiffColorType),
    TiffBitDepth(TiffBitDepth),
    TiffCompressionKind(TiffCompressionKind),
    TiffDeflateLevel(f32),
    TiffPredictor(TiffPredictor),
    TiffLayoutKind(TiffLayoutKind),
    TiffRowsPerStrip(String),
    TiffTileWidth(String),
    TiffTileHeight(String),
    TiffByteOrder(TiffByteOrder),
    TiffBigTiff(bool),
    TiffMultipage(bool),
    TiffEmbedDpi(bool),
    TiffEmbedIcc(bool),
    TiffEmbedExif(bool),
    // Actions
    Export,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct ExportDialog {
    pub format: ExportFormat,
    pub png: PngExportConfig,
    pub tiff: TiffExportConfig,
    pub rows_per_strip_str: String,
    pub tile_width_str: String,
    pub tile_height_str: String,
    pub error: Option<String>,
}

impl Default for ExportDialog {
    fn default() -> Self {
        let tiff = TiffExportConfig::default();
        let rows_per_strip_str = match &tiff.layout {
            TiffLayout::Strip { rows_per_strip } => rows_per_strip.to_string(),
            TiffLayout::Tile { .. } => "8".to_string(),
        };
        let (tile_width_str, tile_height_str) = match &tiff.layout {
            TiffLayout::Tile { tile_width, tile_height } => {
                (tile_width.to_string(), tile_height.to_string())
            }
            TiffLayout::Strip { .. } => ("256".to_string(), "256".to_string()),
        };
        Self {
            format: ExportFormat::Png,
            png: PngExportConfig::default(),
            tiff,
            rows_per_strip_str,
            tile_width_str,
            tile_height_str,
            error: None,
        }
    }
}

impl ExportDialog {
    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::FormatChanged(f) => self.format = f,
            // PNG
            Msg::PngColorType(v) => self.png.color_type = v,
            Msg::PngBitDepth(v) => self.png.bit_depth = v,
            Msg::PngCompressionPreset(p) => {
                self.png.compression = match p {
                    PngCompressionPreset::None => PngCompression::None,
                    PngCompressionPreset::Fast => PngCompression::Fast,
                    PngCompressionPreset::Default => PngCompression::Default,
                    PngCompressionPreset::Best => PngCompression::Best,
                    PngCompressionPreset::Custom => {
                        let prev = match self.png.compression {
                            PngCompression::Level(l) => l,
                            _ => 6,
                        };
                        PngCompression::Level(prev)
                    }
                };
            }
            Msg::PngDeflateLevel(v) => {
                if matches!(self.png.compression, PngCompression::Level(_)) {
                    self.png.compression = PngCompression::Level(v as u8);
                }
            }
            Msg::PngFilter(v) => self.png.filter = v,
            Msg::PngInterlace(v) => self.png.interlace = v,
            Msg::PngEmbedDpi(v) => self.png.embed_dpi = v,
            Msg::PngEmbedIcc(v) => self.png.embed_icc = v,
            // TIFF
            Msg::TiffColorType(v) => self.tiff.color_type = v,
            Msg::TiffBitDepth(v) => self.tiff.bit_depth = v,
            Msg::TiffCompressionKind(k) => {
                let prev_pred = current_tiff_predictor(&self.tiff.compression);
                let prev_lvl = current_deflate_level(&self.tiff.compression);
                self.tiff.compression = match k {
                    TiffCompressionKind::None => TiffCompression::None,
                    TiffCompressionKind::PackBits => TiffCompression::PackBits,
                    TiffCompressionKind::Lzw => {
                        TiffCompression::Lzw { predictor: prev_pred }
                    }
                    TiffCompressionKind::Deflate => TiffCompression::Deflate {
                        level: prev_lvl,
                        predictor: prev_pred,
                    },
                };
            }
            Msg::TiffDeflateLevel(v) => {
                if let TiffCompression::Deflate { predictor, .. } = self.tiff.compression {
                    self.tiff.compression =
                        TiffCompression::Deflate { level: v as u8, predictor };
                }
            }
            Msg::TiffPredictor(p) => match self.tiff.compression {
                TiffCompression::Lzw { .. } => {
                    self.tiff.compression = TiffCompression::Lzw { predictor: p };
                }
                TiffCompression::Deflate { level, .. } => {
                    self.tiff.compression = TiffCompression::Deflate { level, predictor: p };
                }
                _ => {}
            },
            Msg::TiffLayoutKind(k) => {
                self.tiff.layout = match k {
                    TiffLayoutKind::Strip => {
                        let rps = self.rows_per_strip_str.parse().unwrap_or(8);
                        TiffLayout::Strip { rows_per_strip: rps }
                    }
                    TiffLayoutKind::Tile => {
                        let tw = self.tile_width_str.parse().unwrap_or(256);
                        let th = self.tile_height_str.parse().unwrap_or(256);
                        TiffLayout::Tile { tile_width: tw, tile_height: th }
                    }
                };
            }
            Msg::TiffRowsPerStrip(s) => {
                self.rows_per_strip_str = s.clone();
                if let Ok(v) = s.parse::<u32>()
                    && v > 0 {
                        self.tiff.layout = TiffLayout::Strip { rows_per_strip: v };
                    }
            }
            Msg::TiffTileWidth(s) => {
                self.tile_width_str = s.clone();
                if let (Ok(w), Ok(h)) =
                    (s.parse::<u32>(), self.tile_height_str.parse::<u32>())
                {
                    self.tiff.layout = TiffLayout::Tile { tile_width: w, tile_height: h };
                }
            }
            Msg::TiffTileHeight(s) => {
                self.tile_height_str = s.clone();
                if let (Ok(w), Ok(h)) =
                    (self.tile_width_str.parse::<u32>(), s.parse::<u32>())
                {
                    self.tiff.layout = TiffLayout::Tile { tile_width: w, tile_height: h };
                }
            }
            Msg::TiffByteOrder(v) => self.tiff.byte_order = v,
            Msg::TiffBigTiff(v) => {
                self.tiff.tiff_variant =
                    if v { TiffVariant::BigTiff } else { TiffVariant::Classic };
            }
            Msg::TiffMultipage(v) => self.tiff.multipage = v,
            Msg::TiffEmbedDpi(v) => self.tiff.embed_dpi = v,
            Msg::TiffEmbedIcc(v) => self.tiff.embed_icc = v,
            Msg::TiffEmbedExif(v) => self.tiff.embed_exif = v,
            Msg::Export | Msg::Cancel => {}
        }
        self.validate();
    }

    fn validate(&mut self) {
        self.error = None;
        match self.format {
            ExportFormat::Tiff => {
                if let TiffLayout::Tile { tile_width, tile_height } = self.tiff.layout
                    && (tile_width % 16 != 0 || tile_height % 16 != 0) {
                        self.error =
                            Some("Tile dimensions must be multiples of 16.".to_string());
                        return;
                    }
                
                if let TiffCompression::Deflate { predictor: TiffPredictor::FloatingPoint, .. }
                | TiffCompression::Lzw { predictor: TiffPredictor::FloatingPoint } =
                    self.tiff.compression
                    && self.tiff.bit_depth != TiffBitDepth::ThirtyTwo {
                        self.error = Some(
                            "Floating-point predictor requires 32-bit float bit depth.".to_string(),
                        );
                    }
            }
            ExportFormat::Png => {}
        }
    }

    #[allow(dead_code)]
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    pub fn encoder_config(&self) -> EncoderConfig {
        match self.format {
            ExportFormat::Png => EncoderConfig::Png(self.png.clone()),
            ExportFormat::Tiff => EncoderConfig::Tiff(self.tiff.clone()),
        }
    }

    pub fn file_extension(&self) -> &'static str {
        match self.format {
            ExportFormat::Png => "png",
            ExportFormat::Tiff => "tiff",
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        view::view(self)
    }
}
