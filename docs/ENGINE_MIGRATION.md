# Roadmap — Próximas Features

Estado atual do `pixors-executor` + `pixors-desktop` e o que falta implementar.

---

## ✅ Já implementado

| Feature | Onde |
|---------|------|
| MIP pipeline (Blur MIP-aware, MipDownsample, MipFilter) | `operation/` |
| CacheWriter + CacheReader (disk cache por MIP) | `sink/cache_writer.rs`, `source/cache_reader.rs` |
| ViewportCache LRU (RAM) + ViewportCacheSink (streaming) | `viewport/tile_cache.rs`, `sink/viewport_cache_sink.rs` |
| Viewport MIP-aware (Camera, TiledTexture, shader) | `viewport/` |
| Pipeline fan-out + DAG compiler | `runtime/pipeline.rs` |
| ImageFileSource (PNG, TIFF, JPEG), ScanLineAccumulator | `source/`, `data_transform/` |

---

## 1. Layer Composition / Blend

**Arquivos:** `operation/composition/` (stubs vazios hoje)

Composition recebe N layers + blend mode e faz merge. Cada layer tem pixel data, opacity e blend mode.

```
Layer0 ──┐
Layer1 ──┼── Blend ──► Output
Layer2 ──┘
```

Como o pipeline é streaming (tile-by-tile), o stage de composição recebe tiles de um layer por vez e compõe sobre um buffer acumulador via alpha blending.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Composition {
    pub width: u32,
    pub height: u32,
    pub blend_mode: BlendMode,
    pub opacity: f32,
}
```

**Ports:** 1 input Tile, 1 output Tile.  
**Hints:** `ReadTransform`, `prefers_gpu: false`.

---

## 2. TIFF — Leitura na Pipeline + Escrita + Decode de Layers

**Arquivos:** `model/io/tiff.rs` (lê mas não integrado), `source/file_decoder.rs` (PNG-only), `sink/tiff_encoder.rs` (novo)

### Problema atual

O model já tem `TiffFormat` com leitura completa (`read_tiff_rgba8`, `decode_u8_tiff`, `detect_tiff_color_space`, multi-page). Mas **nenhum source da pipeline usa** — tanto `ImageFileSource` quanto `FileDecoder` hardcodam `png::Decoder`.

### Leitura na pipeline

Unificar `FileDecoder` para usar o trait `ImageFormat` e suportar PNG + TIFF + futuros formatos. Ou criar `TiffFileSource` dedicado.

```rust
// FileDecoderRunner::process() — ler via ImageFormat em vez de png::Decoder
let format = detect_format(&self.path)?; // png, tiff, etc.
match format {
    Format::Png  => decode_png(&self.path, emit)?,
    Format::Tiff => decode_tiff(&self.path, emit)?,
}
```

### Escrita (nova)

TIFF encoder para export, padrão `PngEncoder`:

```rust
// pixors-executor/src/sink/tiff_encoder.rs
pub struct TiffEncoder {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}
```

### Decode de layers

TIFFs multi-layer (Photoshop TIFF) precisam extrair layers individuais com nomes, opacidade e blend mode. Hoje o reader lê página plana. Estender `read_tiff_document_metadata` para detectar e expor layers.

---

## 3. PNG Export Pipeline

**Arquivo:** `desktop/src/export.rs` (novo)

Export de imagem processada via pipeline dedicada:

```rust
pub fn export_png(image: &ImageFile, output: &Path) -> Result<(), String> {
    let mut graph = ExecGraph::new();
    let src  = graph.add_stage(StageNode::Source(SourceNode::ImageFile(image.source(0))));
    let sink = graph.add_stage(StageNode::Sink(SinkNode::PngEncoder(PngEncoder::new(output, w, h))));
    graph.add_edge(src, sink, EdgePorts::default());
    Pipeline::compile(&graph)?.run(None)?
}
```

Mesmo padrão para TIFF export.

---

## 4. ColorConvert (Real Implementation)

**Arquivo:** `operation/color/` — tem `target: String` stub, precisa usar `ColorSpace` real.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub source: ColorSpace,
    pub target: ColorSpace,
}
```

Usa a API existente do model (`ColorSpace::to_linear`, `ColorSpace::from_linear`, `convert`). Opera tile-by-tile sobre `Buffer::Cpu`.

---

## 5. Desktop Editor State

**Arquivos:** `desktop/src/editor.rs` + `desktop/src/editor/` (novos)

O state central do editor que serve de base para todos os painéis, histórico e ações. É o modelo que o executor modifica e os painéis observam.

```rust
pub struct EditorState {
    pub image: Option<Arc<ImageHandle>>,     // imagem aberta + path + cache dir
    pub layers: Vec<LayerModel>,             // layers do documento
    pub active_layer: usize,
    pub history: History<EditorAction>,      // undo/redo
    pub viewport: ViewportState,             // camera + mip (já existe)
    pub status: StatusState,                 // dimensões, zoom %, tool ativa
}

pub struct LayerModel {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub source_path: Option<PathBuf>,        // para reload do disco
}

pub enum EditorAction {
    OpenImage { path: PathBuf },
    ApplyBlur { radius: u32 },
    CompositeLayer { from: usize, to: usize, mode: BlendMode, opacity: f32 },
    // ...
}
```

Os painéis (layers, filters, toolbar) leem desse state e disparam `EditorAction`. O `update()` do App faz dispatch das ações → pipeline → resultado.

---

## 6. Blur Preview + Apply (Painel de Filtros)

**Arquivos:** `desktop/src/components/filters_panel.rs` (já existe, decorativo)

O painel de filtros hoje é estático. Fluxo desejado:

1. Usuário seleciona blur, ajusta radius via slider
2. **Preview mode**: pipeline leve roda só na região visível do viewport (tiles no MIP atual)
3. Preview é mostrado como overlay ou diretamente no viewport
4. Usuário clica **Apply** → pipeline completa roda na imagem inteira → resultado commitado no histórico

```
Preview pipeline:
  ImageFileSource(region) → Blur(radius) → ViewportCacheSink

Apply pipeline:
  ImageFileSource(full) → Blur(radius) → [CacheWriter] → [history commit]
```

---

## 7. Painel de Layers Funcional

**Arquivo:** `desktop/src/components/layers_panel.rs` (decorativo hoje)

- Listar layers do `EditorState.layers`
- Reordenar (drag), toggle visibility, ajustar opacity
- Selecionar layer ativo → toolbar opera sobre ele
- "New Layer", "Delete Layer", "Merge Down"
- Blend mode dropdown por layer

---

## 8. Abas Funcionais

**Arquivo:** `desktop/src/components/tab_bar.rs` (decorativo hoje)

- Cada aba = uma imagem aberta (múltiplos documentos)
- Clique na aba → troca o `EditorState` ativo
- "X" fecha documento
- "+" abre novo (file dialog)
- Estado atual só tem 1 imagem; precisa virar `Vec<EditorState>` ou `HashMap<DocumentId, EditorState>`

---

## 9. Error Surface & Propagação

**Arquivos:** `desktop/src/app.rs`, `executor/src/error.rs`

Hoje erros são logados (`tracing::error!`) mas não chegam na UI de forma estruturada.

- `PipelineEvent::Error` já existe no executor — precisa ser consumido pelo desktop
- Adicionar `EditorState.errors: Vec<EditorError>` com timestamp, severidade, mensagem
- Exibir errors no `status_bar` como toast/pill temporário
- Pipeline errors propagados via canal (`Pipeline::run(events)`) → `Msg::PipelineError`

---

## Summary

| # | Feature | Complexidade |
|---|---------|-------------|
| 1 | Composition/Blend | Média — multi-input, acumulador |
| 2 | TIFF write + layer decode | Baixa (write) / Média (decode) |
| 3 | PNG export pipeline | Baixa |
| 4 | ColorConvert real | Baixa — usar color model existente |
| 5 | Desktop Editor State | Média — arquitetura central |
| 6 | Blur preview + apply panel | Média — pipeline preview + apply |
| 7 | Layers panel funcional | Média — arrastar, opacidade, blend |
| 8 | Tabs funcionais | Baixa — múltiplos documentos |
| 9 | Error surface | Baixa — propagação + UI |
