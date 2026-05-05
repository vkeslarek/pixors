# Roadmap — Próximas Features

Estado atual do `pixors-executor` + `pixors-desktop` e o que falta implementar.

---

## ✅ Já implementado

| Feature | Onde |
|---|---|
| MIP pipeline (Blur MIP-aware, MipDownsample, MipFilter) | `operation/` |
| CacheWriter + CacheReader (disk cache por MIP) | `sink/cache_writer.rs`, `source/cache_reader.rs` |
| ViewportCache LRU (RAM) + ViewportCacheSink (streaming) | `viewport/tile_cache.rs`, `sink/viewport_cache_sink.rs` |
| Viewport MIP-aware (Camera, TiledTexture, shader) | `viewport/` |
| Pipeline fan-out + DAG compiler | `runtime/pipeline.rs` |
| Image → ImageDesc + PageStream + ImageDecoder trait | `model/image/desc.rs`, `decoder.rs`, `image.rs` |
| PngDecoder + TiffDecoder (multi-page, todos bit depths) | `model/io/png.rs`, `model/io/tiff.rs` |
| ImageStreamSource (pipeline source genérico) | `source/image_stream.rs` |
| ScanLineToTile, TileToScanline, TileToNeighborhood, TileToTileBlock | `data_transform/` |
| Compose multi-input com alpha-over + BlendMode | `operation/compose.rs` |
| ColorConvert real com ColorConversion engine (formato + espaço de cor) | `operation/color.rs` |
| ProcessorContext (port, device, emit) + helpers (take_tile, ensure_cpu) | `stage.rs` |
| Variable-length ports (PortGroup::Variable) + validação | `stage.rs` |
| delegate_stage! macro (enum dispatch por módulo) | `stage.rs` |
| PixelFormat expandido (17 variantes: Gray8→RgbaF32) | `model/pixel/format.rs` |
| Serde em ColorSpace, RgbPrimaries, WhitePoint, TransferFn | `model/color/` |

---

## ✅ Concluídos

### 1. ~~Layer Composition / Blend~~ ✅

Implementado como `Compose` em `operation/compose.rs`. Multi-input via `PortGroup::Variable`, alpha-over blending (Porter-Duff "over"), `BlendMode` configurável por layer. Pipeline em `file_ops.rs` compõe duas imagens antes do MipDownsample.

### 2. ~~TIFF — Leitura na Pipeline~~ ✅

`TiffDecoder` em `model/io/tiff.rs` implementa `ImageDecoder`. Suporta TIFF multi-page, todos os formatos de cor (U8/U16/U32/F32), detecção de color space via EXIF tags, DPI, orientação, ICC profile. Integrado na pipeline via `Image::open()` → `ImageStreamSource`.

### 4. ~~ColorConvert (Real Implementation)~~ ✅

`ColorConvert` em `operation/color.rs` usa `ColorConversion` + `convert_pixels()` com LUTs precomputados. Converte qualquer formato de entrada (Rgba8, Rgb8, Gray8, GrayA8, Rgba16, RgbaF16, RgbaF32, etc.) para `target_format` + `target_color_space`. Node único para Tile, parâmetros `target_format: PixelFormat` + `target_color_space: ColorSpace`.

---

## 3. PNG + TIFF Export Pipeline

**Arquivos:** `desktop/src/export.rs` (novo), `sink/png_encoder.rs` (existe), `sink/tiff_encoder.rs` (novo)

### Leitura: ✅
PNG e TIFF lidos via `ImageDecoder` trait (`PngDecoder`, `TiffDecoder`), integrados na pipeline.

### Escrita: ❌
Export de imagem processada via pipeline dedicada:

```rust
pub fn export_png(image: &Image, output: &Path) -> Result<(), String> {
    let mut graph = ExecGraph::new();
    let src  = graph.add_stage(StageNode::Source(SourceNode::ImageStream(...)));
    let sink = graph.add_stage(StageNode::Sink(SinkNode::PngEncoder(PngEncoder::new(output))));
    graph.add_edge(src, sink, EdgePorts::default());
    Pipeline::compile(&graph)?.run(None)?
}
```

Mesmo padrão para TIFF export (`TiffEncoder`). Ambos precisam ser implementados.

---

## 5. Desktop Editor State

**Arquivos:** `desktop/src/editor.rs` + `desktop/src/editor/` (novos)

O state central do editor que serve de base para todos os painéis, histórico e ações. É o modelo que o executor modifica e os painéis observam.

```rust
pub struct EditorState {
    pub image: Option<Arc<ImageHandle>>,
    pub layers: Vec<LayerModel>,
    pub active_layer: usize,
    pub history: History<EditorAction>,
    pub viewport: ViewportState,
    pub status: StatusState,
}

pub struct LayerModel {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
}

pub enum EditorAction {
    OpenImage { path: PathBuf },
    ApplyBlur { radius: u32 },
    CompositeLayer { from: usize, to: usize, mode: BlendMode, opacity: f32 },
}
```

Painéis leem desse state e disparam `EditorAction`. O `update()` do App faz dispatch → pipeline → resultado.

---

## 6. Blur Preview + Apply (Painel de Filtros)

**Arquivos:** `desktop/src/components/filters_panel.rs`

Painel de filtros hoje é estático. Fluxo desejado:

1. Usuário seleciona blur, ajusta radius via slider
2. **Preview mode**: pipeline leve roda só na região visível (tiles no MIP atual)
3. Preview como overlay ou direto no viewport
4. **Apply** → pipeline completa na imagem inteira → commit no histórico

---

## 7. Painel de Layers Funcional

**Arquivo:** `desktop/src/components/layers_panel.rs`

- Listar layers do `EditorState.layers`
- Reordenar (drag), toggle visibility, ajustar opacity
- Selecionar layer ativo → toolbar opera sobre ele
- "New Layer", "Delete Layer", "Merge Down"
- Blend mode dropdown por layer

---

## 8. Abas Funcionais

**Arquivo:** `desktop/src/components/tab_bar.rs`

- Cada aba = uma imagem aberta (múltiplos documentos)
- Clique → troca `EditorState` ativo
- "X" fecha, "+" abre novo
- Estado atual só tem 1 imagem; precisa virar `Vec<EditorState>`

---

## 9. Error Surface & Propagação

**Arquivos:** `desktop/src/app.rs`, `executor/src/error.rs`

Hoje erros são logados mas não chegam na UI:
- `PipelineEvent::Error` já existe
- Adicionar `EditorState.errors: Vec<EditorError>` com timestamp, severidade, mensagem
- Exibir no `status_bar` como toast/pill temporário

---

## Summary

| # | Feature | Status |
|---|---------|--------|
| 1 | Composition/Blend | ✅ |
| 2 | TIFF read + decode | ✅ (read), ❌ (write) |
| 3 | PNG + TIFF export pipeline | ❌ |
| 4 | ColorConvert real | ✅ |
| 5 | Desktop Editor State | ❌ |
| 6 | Blur preview + apply panel | ❌ |
| 7 | Layers panel funcional | ❌ |
| 8 | Tabs funcionais | ❌ |
| 9 | Error surface | ❌ |
