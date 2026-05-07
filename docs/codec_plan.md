# Codec Plan — PNG & TIFF Encoder Interface

## Contexto da Arquitetura

O export é um **`Consumer`** (de `stage/actors.rs`) que recebe `Item::Tile` do pipeline.
Tiles chegam fora de ordem e em paralelo — o encoder monta o buffer completo internamente
e só escreve o arquivo em `finish()`.

```
Pipeline (tiles) ──► [EncoderConsumer] ──► assembles tiles ──► finish() ──► arquivo
                                ▲
                         ExportConfig (serde'd da dialog)
```

A camada de codec (`ImageEncoder` trait) é **separada** do stage — o `Consumer` orquestra
tiles e delega a escrita ao codec. Isso permite testar o codec sem pipeline.

---

## Trait `ImageEncoder` (codec.rs)

```rust
/// Simétrico ao `ImageDecoder`. Recebe buffer já montado + config e escreve.
pub trait ImageEncoder: Send + Sync {
    /// Retorna true se este encoder suporta a extensão do path.
    fn probe(&self, path: &Path) -> bool;

    /// Escreve `data` (pixels raw, row-major, sem padding) no `path`.
    /// `meta` descreve o formato do buffer (PixelFormat, ColorSpace, AlphaPolicy).
    /// `desc` tem width/height/dpi/icc_profile/metadata da imagem.
    fn encode(
        &self,
        path: &Path,
        data: &[u8],
        desc: &EncoderDescriptor,
        config: &EncoderConfig,
    ) -> Result<(), Error>;
}

/// Tudo que o encoder precisa saber sobre a imagem (independente de config).
pub struct EncoderDescriptor {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,   // formato real do buffer que chega
    pub color_space: ColorSpace,     // espaço de cor dos dados
    pub alpha_policy: AlphaPolicy,
    pub dpi: Option<Dpi>,
    pub icc_profile: Option<Vec<u8>>,
    pub metadata: Vec<Metadata>,
}

/// Config escolhida pelo usuário no dialog — serializable para persistência/presets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum EncoderConfig {
    Png(PngExportConfig),
    Tiff(TiffExportConfig),
}
```

---

## `PngExportConfig`

Cobre todas as opções expostas pela crate `png`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngExportConfig {
    // ── Pixel ──────────────────────────────────────────────────────────────────
    /// Profundidade de saída. O encoder converte o buffer de entrada se necessário.
    pub bit_depth: PngBitDepth,
    /// Tipo de cor de saída.
    pub color_type: PngColorType,

    // ── Compressão ─────────────────────────────────────────────────────────────
    /// Nível de compressão deflate (0 = sem compressão, 9 = máximo).
    /// Default: 6 (balanceado).
    pub compression: PngCompression,
    /// Filtro aplicado por linha antes da compressão.
    pub filter: PngFilter,
    /// Entrelaçamento Adam7 (para progressive display). Default: None.
    pub interlace: PngInterlace,

    // ── Metadados ──────────────────────────────────────────────────────────────
    /// Emite chunk pHYs com DPI da imagem (usa EncoderDescriptor.dpi se None).
    /// false = sem chunk pHYs.
    pub embed_dpi: bool,
    /// Emite chunk iCCP com perfil ICC (usa EncoderDescriptor.icc_profile se Some).
    pub embed_icc: bool,
    /// Emite chunk sRGB com rendering intent (mútuo exclusivo com iCCP para viewers).
    pub srgb_intent: Option<PngSrgbIntent>,
    /// Emite chunk gAMA (gamma). None = sem chunk.
    /// Geralmente desnecessário quando iCCP ou sRGB estão presentes.
    pub gamma: Option<f64>,
    /// Chunks de texto (tEXt / zTXt / iTXt). Vec vazia = sem chunks.
    pub text_chunks: Vec<PngTextChunk>,

    // ── APNG (animação) ────────────────────────────────────────────────────────
    /// Configuração de animação. None = PNG estático.
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

// ── Enums de opções ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngBitDepth {
    One,    // grayscale only
    Two,    // grayscale only
    Four,   // grayscale only
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
    // Indexed excluído do export — requer quantização (fora de escopo)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngCompression {
    None,      // level 0
    Fast,      // level 1
    Default,   // level 6
    Best,      // level 9
    Level(u8), // 0–9 para usuário avançado
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngFilter {
    None,
    Sub,
    Up,
    Average,
    Paeth,
    Adaptive, // encoder escolhe melhor filtro por linha (default para fotos)
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
    pub keyword: String,  // max 79 bytes, Latin-1
    pub text: String,
    pub encoding: PngTextEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngTextEncoding {
    Text,   // tEXt — Latin-1, sem compressão
    Ztxt,   // zTXt — Latin-1, comprimido
    Itxt,   // iTXt — UTF-8, comprimido opcional
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngAnimationConfig {
    /// 0 = loop infinito.
    pub num_plays: u32,
    /// Um frame por página da imagem (PageInfo.delay_ms e dispose).
    /// O encoder lê PageInfo do EncoderDescriptor.
    pub frames: Vec<PngFrameConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngFrameConfig {
    pub delay_numerator: u16,    // delay_ms → (ms, 1000)
    pub delay_denominator: u16,  // sempre 1000 se gerado de delay_ms
    pub dispose_op: PngDisposeOp,
    pub blend_op: PngBlendOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngDisposeOp { None, Background, Previous }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngBlendOp { Source, Over }
```

---

## `TiffExportConfig`

Cobre todas as opções da spec TIFF 6.0 + BigTIFF.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TiffExportConfig {
    // ── Pixel ──────────────────────────────────────────────────────────────────
    /// Profundidade de bits por sample.
    pub bit_depth: TiffBitDepth,
    /// Modelo de cor de saída.
    pub color_type: TiffColorType,

    // ── Compressão ─────────────────────────────────────────────────────────────
    pub compression: TiffCompression,

    // ── Layout ─────────────────────────────────────────────────────────────────
    /// Strip (padrão) ou Tile (melhor para acesso randômico).
    pub layout: TiffLayout,
    /// Chunky (RGBRGB…) ou Planar (RRR…GGG…BBB…).
    pub planar: TiffPlanar,

    // ── Arquivo ────────────────────────────────────────────────────────────────
    /// BigTIFF para arquivos > 4 GB. Default: Classic.
    pub tiff_variant: TiffVariant,
    /// Byte order. Default: LittleEndian (Intel II).
    pub byte_order: TiffByteOrder,

    // ── Metadados ──────────────────────────────────────────────────────────────
    pub embed_dpi: bool,
    pub embed_icc: bool,
    /// Orientação EXIF. Default: Identity.
    pub orientation: Orientation,
    /// Embute EXIF sub-IFD com dados extras (DateTimeOriginal, etc).
    pub embed_exif: bool,
    /// Campos de metadado texto (Make, Model, Software, DateTime, Artist, Copyright,
    /// ImageDescription). None = não emite a tag.
    pub tags: TiffMetaTags,

    // ── Multi-página ───────────────────────────────────────────────────────────
    /// Se true, exporta todas as páginas em um único arquivo TIFF multi-página.
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
            planar: TiffPlanar::Chunky,
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

// ── Enums de opções ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffBitDepth {
    Eight,       // u8
    Sixteen,     // u16
    ThirtyTwo,   // f32
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
    CieLab,      // Photometric=8; bit_depth=8 → Lab<u8>, 16 → Lab<u16>
}

/// Compressão + predictor onde aplicável.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum TiffCompression {
    None,
    PackBits,
    Lzw {
        predictor: TiffPredictor,
    },
    Deflate {
        /// Nível 1–9. Default: 6.
        level: u8,
        predictor: TiffPredictor,
    },
    Jpeg {
        /// Qualidade 1–100. Default: 85.
        quality: u8,
        // JPEG só suporta RGB/YCbCr/Gray — o encoder valida contra color_type.
    },
}

/// Predictor para LZW e Deflate (reduz entropia antes de comprimir).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffPredictor {
    None,
    Horizontal,        // tag 2 — para u8/u16
    FloatingPoint,     // tag 3 — para f32
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TiffLayout {
    Strip {
        /// Linhas por strip. Default: 8. TIFF recomenda ≥ 8 KB por strip.
        rows_per_strip: u32,
    },
    Tile {
        /// Deve ser múltiplo de 16 (requisito TIFF). Default: 256.
        tile_width: u32,
        tile_height: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffPlanar {
    Chunky,   // PlanarConfiguration=1 (RGBRGB…)
    Planar,   // PlanarConfiguration=2 (RR…GG…BB…)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffVariant {
    Classic,  // max 4 GB, offsets u32
    BigTiff,  // offsets u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TiffByteOrder {
    LittleEndian,   // II (Intel)
    BigEndian,      // MM (Motorola)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TiffMetaTags {
    pub make: Option<String>,
    pub model: Option<String>,
    pub software: Option<String>,
    pub date_time: Option<String>,   // "YYYY:MM:DD HH:MM:SS"
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub image_description: Option<String>,
}
```

---

## Stage: `PngEncoderStage` / `TiffEncoderStage` (sink/)

Cada encoder é um `Stage` com `consumer()`, registrado em `SinkNode`.

```rust
/// Config serializável que vai no grafo de pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngEncoderStage {
    pub path: PathBuf,
    pub config: PngExportConfig,
    // width/height/pixel_format/color_space chegam via tiles — inferidos no consume()
}

impl Stage for PngEncoderStage {
    fn kind(&self) -> &'static str { "png_encoder_v2" }
    fn ports(&self) -> &'static PortSpecification { &ENCODER_PORTS }
    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        Some(Box::new(PngEncoderConsumer::new(
            self.path.clone(),
            self.config.clone(),
        )))
    }
}

// TIFF idêntico com TiffExportConfig
pub struct TiffEncoderStage {
    pub path: PathBuf,
    pub config: TiffExportConfig,
}
```

### `Consumer` interno

```rust
struct PngEncoderConsumer {
    path: PathBuf,
    config: PngExportConfig,
    // ── Estado montado a partir dos tiles ──────────────────────────────────────
    tiles: HashMap<(u32, u32), Vec<u8>>,  // (tx, ty) → pixel data
    width: u32,
    height: u32,
    meta: Option<PixelMeta>,
    // DPI e ICC vêm do primeiro tile (todos têm o mesmo PixelMeta)
}

impl Consumer for PngEncoderConsumer {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = match item { Item::Tile(t) => t, _ => return Ok(()) };
        // Extrai width/height máximos e meta do primeiro tile visto
        self.width = self.width.max(tile.coord.px + tile.coord.width);
        self.height = self.height.max(tile.coord.py + tile.coord.height);
        if self.meta.is_none() { self.meta = Some(tile.meta); }
        // Guarda pixels do tile (CPU buffer, faz download se GPU)
        let data = tile.data.into_cpu()?;
        self.tiles.insert((tile.coord.tx, tile.coord.ty), data);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        // 1. Monta buffer linear row-major a partir dos tiles
        let buffer = assemble_tiles(&self.tiles, self.width, self.height, &self.meta)?;
        // 2. Constrói EncoderDescriptor
        let desc = EncoderDescriptor { /* from self.meta + config */ };
        // 3. Delega ao PngEncoder (codec puro)
        PngEncoder.encode(&self.path, &buffer, &desc, &EncoderConfig::Png(self.config.clone()))
    }
}
```

---

## Validações (encoder → error antes de escrever)

| Restrição | Erro |
|-----------|------|
| JPEG + alpha (RGBA/CMYKA) | `UnsupportedConfig("JPEG compression does not support alpha")` |
| JPEG + CieLab | `UnsupportedConfig("JPEG compression unsupported for Lab color")` |
| BigTIFF requerido mas `TiffVariant::Classic` e tamanho estimado > 4 GB | warning, auto-upgrade |
| `TiffPredictor::FloatingPoint` com `TiffBitDepth::Eight` ou `Sixteen` | `UnsupportedConfig` |
| `PngBitDepth::One/Two/Four` com color type != Grayscale | `UnsupportedConfig` |
| `TiffLayout::Tile` com tile_width/height não múltiplos de 16 | `UnsupportedConfig` |
| `PngSrgbIntent` + `embed_icc: true` | emit ambos (conformes ao spec, viewers usam iCCP) |
| `TiffCompression::Deflate.level` fora de 1–9 | clamp com warning |

---

## Mapping: Dialog → Config

Cada campo do config mapeia para um controle de UI no dialog de export:

### PNG Dialog

| Seção | Campo | Controle |
|-------|-------|---------|
| Output | `color_type` | Select: Grayscale / Grayscale+Alpha / RGB / RGBA |
| Output | `bit_depth` | Select: 8-bit / 16-bit (1/2/4-bit ocultos, só grayscale avançado) |
| Compression | `compression` | Slider 0–9 + presets (None / Fast / Default / Best) |
| Compression | `filter` | Select: Adaptive / None / Sub / Up / Average / Paeth |
| Compression | `interlace` | Toggle: None / Adam7 |
| Metadata | `embed_dpi` | Checkbox |
| Metadata | `embed_icc` | Checkbox |
| Metadata | `srgb_intent` | Select (opcional, avançado) |
| Metadata | `gamma` | Number input (opcional, avançado) |
| Metadata | `text_chunks` | Lista editável key/value |
| Animation | `animation` | Seção expansível (só aparece se imagem tem > 1 página) |

### TIFF Dialog

| Seção | Campo | Controle |
|-------|-------|---------|
| Output | `color_type` | Select: Gray / Gray+Alpha / RGB / RGBA / CMYK / CMYK+Alpha / CIE Lab |
| Output | `bit_depth` | Select: 8-bit / 16-bit / 32-bit float |
| Compression | `compression` | Select: None / PackBits / LZW / Deflate / JPEG |
| Compression | `predictor` (LZW/Deflate) | Select: None / Horizontal / Float (condicional) |
| Compression | `level` (Deflate) | Slider 1–9 (condicional) |
| Compression | `quality` (JPEG) | Slider 1–100 (condicional) |
| Layout | `layout` | Radio: Strip / Tile |
| Layout | `rows_per_strip` | Number (condicional: Strip) |
| Layout | `tile_width/height` | Number × Number (condicional: Tile) |
| Layout | `planar` | Toggle: Chunky / Planar |
| File | `tiff_variant` | Toggle: Classic / BigTIFF |
| File | `byte_order` | Toggle: Little-endian / Big-endian |
| Metadata | `embed_dpi` / `embed_icc` | Checkbox × 2 |
| Metadata | `orientation` | Select 8 orientações |
| Metadata | `embed_exif` | Checkbox |
| Metadata | `tags.*` | Form com campos opcionais |
| Pages | `multipage` | Toggle (só aparece se imagem tem > 1 página) |

---

## Localização dos Arquivos

```
pixors-executor/src/
├── common/image/
│   └── codec.rs              ← adicionar: ImageEncoder, EncoderConfig,
│                               PngExportConfig, TiffExportConfig (e todos os enums)
│                               EncoderDescriptor
├── sink/
│   ├── mod.rs                ← adicionar PngEncoderStage, TiffEncoderStage ao SinkNode
│   ├── png_encoder.rs        ← refactor: manter PngEncoder legacy, adicionar PngEncoderStage v2
│   └── tiff_encoder.rs       ← novo: TiffEncoderStage + TiffEncoderConsumer + TiffEncoder codec
```

---

## Ordem de Implementação Sugerida

1. **`codec.rs`** — `ImageEncoder` trait + `EncoderDescriptor` + todos os config structs/enums (sem lógica)
2. **`TiffEncoder`** (codec puro) — escreve strip/chunky, None/LZW/Deflate, 8/16/f32, RGB/RGBA/CMYK/Gray
3. **`TiffEncoderStage`** — Consumer que monta tiles e chama TiffEncoder
4. **`PngEncoderStage` v2** — refactor do existente, adiciona config completo
5. Validações + testes unitários (roundtrip: encode → decode → compare pixels)
6. **Presets** — `PngExportConfig::for_web()`, `TiffExportConfig::for_print()`, `TiffExportConfig::for_archive()`
