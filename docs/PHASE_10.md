# Phase 10 — First Complete Editing Loop

**Goal:** a user can open an image, toggle layer visibility, adjust opacity, apply a
per-layer blur filter with a live preview, and export the composed result.  
Every piece must be wired end-to-end — no stubs, no hardcoded values.

---

## Scope overview

| Area | What ships |
|---|---|
| Layer UX | Select, visibility toggle, opacity (0–100%) wired to composite |
| Per-layer filter stack | `new_filter.rs` panel wired; Blur is the only filter op |
| Display composite pipeline | All visible layers → Compose → ViewportSink |
| Export composite pipeline | Same layer stack → encode to disk |
| Checkerboard viewport | Transparency pattern behind composed result |
| Format decode | JPEG decode (via `image-rs`) plugged into `ImageStreamSource` |
| Blend modes | **Deferred** — Normal/alpha-over is sufficient for Phase 10 |

---

## 1 · State model changes (`pixors-state`)

### 1.1 · `Layer` gains a filter stack

**File:** `pixors-state/src/tab.rs`

```rust
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,           // 0.0..=1.0
    pub blend: BlendMode,
    pub source: LayerSource,
    pub filters: Vec<LayerFilter>,  // NEW — ordered filter stack
}

#[derive(Debug, Clone)]
pub enum LayerFilter {
    Blur { radius: f32 },  // only op in Phase 10; enum makes it extensible
}
```

Remove `FilterState` from `Tab` entirely — it belonged on `Layer` all along.

### 1.2 · `Tab` no longer has `filter: FilterState`

Remove the field. Any callers in `pixors-desktop/src/controller.rs` that read
`tab.filter.blur_radius` must now read from the active layer's filter stack.

### 1.3 · `BlendMode` unification

`pixors-state/src/tab.rs` currently defines its own `BlendMode { Normal, Multiply }`.
`pixors-image/src/image.rs` defines a separate `BlendMode { Normal, Source, Over }`.
`pixors-ops/src/processor/compose.rs` imports from `pixors-image`.

Consolidate: move `BlendMode` to `pixors-engine` (it is a pipeline concept, not an
image-format concept). Both `pixors-image` and `pixors-state` re-export from there.

```rust
// pixors-engine/src/common/blend.rs  (new file)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,   // Porter-Duff alpha-over
    Source,   // replace — used by APNG first-frame semantics
}
```

`Multiply` and other creative modes are Phase 11+.  
`Over` is an alias for `Normal` in the compose logic; keep `Source` for the PNG decoder path.

---

## 2 · New actions (`pixors-state/src/action/actions/`)

### 2.1 · `SetLayerVisibility`

```rust
// pixors-state/src/action/actions/set_layer_visibility.rs
#[derive(Debug)]
pub struct SetLayerVisibility {
    pub tab: TabId,
    pub layer: LayerId,
    pub visible: bool,
}

impl Action for SetLayerVisibility {
    fn target_tab(&self) -> Option<TabId> { Some(self.tab) }
    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        Ok(PreparedAction::StateOnly)
    }
    fn apply(&self, state: &mut EditorState, _status: PipelineStatus) {
        if let Some(tab) = state.tab_mut(self.tab) {
            if let Some(layer) = tab.layers.iter_mut().find(|l| l.id == self.layer) {
                layer.visible = self.visible;
                tab.redraw_seq += 1;
            }
        }
    }
    fn undo(&self, state: &mut EditorState) {
        // toggle back
        if let Some(tab) = state.tab_mut(self.tab) {
            if let Some(layer) = tab.layers.iter_mut().find(|l| l.id == self.layer) {
                layer.visible = !self.visible;
                tab.redraw_seq += 1;
            }
        }
    }
}
```

### 2.2 · `SetLayerOpacity`

```rust
#[derive(Debug)]
pub struct SetLayerOpacity {
    pub tab: TabId,
    pub layer: LayerId,
    pub opacity: f32,       // 0.0..=1.0
    pub prev_opacity: f32,  // for undo
}
// prepare → StateOnly; apply/undo mutate layer.opacity + tab.redraw_seq += 1
```

### 2.3 · `SelectLayer`

```rust
#[derive(Debug)]
pub struct SelectLayer {
    pub tab: TabId,
    pub layer: LayerId,
}
// prepare → StateOnly; apply → tab.active_layer = Some(self.layer); no undo needed
// record_in_history → false
```

### 2.4 · `SetLayerFilter`

```rust
#[derive(Debug)]
pub struct SetLayerFilter {
    pub tab: TabId,
    pub layer: LayerId,
    pub filter_index: usize,
    pub filter: LayerFilter,
    pub prev_filter: LayerFilter,  // for undo
}
// prepare → StateOnly; apply → mutate layer.filters[filter_index] + tab.redraw_seq += 1
```

### 2.5 · `AddLayerFilter` / `RemoveLayerFilter`

```rust
#[derive(Debug)]
pub struct AddLayerFilter {
    pub tab: TabId,
    pub layer: LayerId,
    pub filter: LayerFilter,
}
// apply → layer.filters.push(filter); undo → layer.filters.pop()

#[derive(Debug)]
pub struct RemoveLayerFilter {
    pub tab: TabId,
    pub layer: LayerId,
    pub index: usize,
    pub removed: LayerFilter,  // captured at prepare time for undo
}
```

All new actions export from `action/actions/mod.rs`.

---

## 3 · Display composite pipeline (`pixors-desktop`)

### 3.1 · The problem today

`run_mip_fetch` in `controller.rs` builds a graph:
```
CacheReader → ViewportSink
```
It assumes a single layer reading from disk. Visibility, opacity, and filters are
ignored completely.

### 3.2 · New graph shape

For a tab with N layers:

```
[for each visible layer i]
  CacheReader(layer_i cache_dir, mip, range)
  → per-layer filter chain (zero or more: ColorConvert→Blur→ColorConvert for each Blur filter)
  → [one input into Compose]

Compose(layer_count=visible_count, blend_modes=[...], opacities=[...])
  → ColorConvert(Rgba8, sRGB)
  → ViewportSink
```

#### 3.2.1 · `Compose` must learn about opacity

Currently `Compose` only takes blend modes. Add `opacities: Vec<f32>` so it can
pre-multiply each layer tile before blending.

**File:** `pixors-ops/src/processor/compose.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compose {
    pub layer_count: u16,
    pub blend_modes: Vec<BlendMode>,
    pub opacities: Vec<f32>,   // NEW — one per layer, 0.0..=1.0, default 1.0
}
```

In the per-pixel loop, before calling `blend()`, pre-multiply the top layer's alpha
by `opacities[port]`:

```rust
let effective_alpha = (src[3] as f32 * opacities[port]) as u8;
let src_with_opacity = [src[0], src[1], src[2], effective_alpha];
result = blend(&src_with_opacity, &result, mode);
```

#### 3.2.2 · Graph builder helper in `controller.rs`

```rust
fn build_display_graph(
    tab: &Tab,
    mip: u32,
    range: Option<TileRange>,
    img_w: u32,
    img_h: u32,
) -> ExecGraph {
    let visible: Vec<&Layer> = tab.layers.iter().filter(|l| l.visible).collect();

    if visible.is_empty() {
        // return a "black tile" graph — just a source that emits empty tiles
        // Simplest: single CacheReader from the first layer even if invisible,
        // this edge case is acceptable for Phase 10
        panic!("no visible layers — handle in caller");
    }

    let tile_range = range;
    let mut builder = PathBuilder::new();  // this needs rework — PathBuilder is 1-input-chain

    // PathBuilder is a linear chain tool. For multi-input Compose we must
    // build ExecGraph directly. See §3.2.3.
    todo!()
}
```

#### 3.2.3 · Build `ExecGraph` directly for multi-input compose

`PathBuilder` only builds linear chains. For compositing we need multiple source
branches merging into `Compose`. Use `ExecGraph` directly:

```rust
use pixors_engine::graph::graph::{ExecGraph, EdgePorts};
use pixors_engine::stage::Stage;

fn build_display_graph(tab: &Tab, mip: u32, range: Option<TileRange>) -> ExecGraph {
    let mut graph = ExecGraph::new();

    let visible_layers: Vec<&Layer> = tab.layers.iter().filter(|l| l.visible).collect();
    let n = visible_layers.len();

    let compose_id = graph.add_stage(Arc::new(Compose {
        layer_count: n as u16,
        blend_modes: visible_layers.iter().map(|_| BlendMode::Normal).collect(),
        opacities: visible_layers.iter().map(|l| l.opacity).collect(),
    }));

    let color_out_id = graph.add_stage(Arc::new(ColorConvert {
        target_format: PixelFormat::Rgba8,
        target_color_space: ColorSpace::SRGB,
        target_alpha: AlphaPolicy::Straight,
    }));
    graph.connect(compose_id, "composed", color_out_id, "image");

    let sink_id = graph.add_stage(Arc::new(TileCacheSource { ... }));  // ViewportSink
    graph.connect(color_out_id, "image", sink_id, "tile");

    for (i, layer) in visible_layers.iter().enumerate() {
        let reader_id = graph.add_stage(Arc::new(CacheReader {
            cache_dir: layer_cache_dir(tab, layer),
            mip_level: mip,
            tile_range: range.clone(),
        }));

        let mut prev_id = reader_id;
        let mut prev_port = "tile";

        for filter in &layer.filters {
            match filter {
                LayerFilter::Blur { radius } => {
                    // ColorConvert to working space → Blur → ColorConvert back
                    let cc_in = graph.add_stage(Arc::new(ColorConvert { /* to ACEScg f16 */ }));
                    graph.connect(prev_id, prev_port, cc_in, "image");

                    let blur = graph.add_stage(Arc::new(Blur { radius: *radius, .. }));
                    graph.connect(cc_in, "image", blur, "neighborhood");

                    let cc_out = graph.add_stage(Arc::new(ColorConvert { /* to Rgba8 sRGB */ }));
                    graph.connect(blur, "tile", cc_out, "image");

                    prev_id = cc_out;
                    prev_port = "image";
                }
            }
        }

        // connect this layer's output to compose input port i
        graph.connect_to_port(prev_id, prev_port, compose_id, "layers", i);
    }

    graph
}
```

> **Note on `ExecGraph` API**: Check whether `ExecGraph` already supports
> multi-port connections (`connect_to_port` / variable input port index).
> `Compose` uses `PortGroup::Variable` so the engine already handles this.
> The connection API may be `graph.connect(src, src_port, dst, dst_port)` where
> variable-input ports are matched by order of connection. Verify against
> `pixors-engine/src/graph/graph.rs` before implementing.

#### 3.2.4 · Per-layer cache directory

Currently a single `Tab.cache_dir` holds all MIP tiles. For multi-layer files, each
layer needs its own subdirectory:

```
{cache_dir}/layer_{layer_id_hex}/mip_{N}/tile_{tx}_{ty}.lz4
```

For single-page images (the Phase 10 common case), `layer_0/` holds the only content.
`OpenFile` already writes to `tab.cache_dir`; update the `CacheWriter` path to include
the layer subdirectory. The `LayerSource::FilePage { page }` field maps 1:1 to the
page written by the decoder.

**File to update:** `pixors-state/src/action/actions/open_file.rs`

Change `CacheWriter` destination from `tab.cache_dir` to
`tab.cache_dir.join(format!("layer_{:016x}", layer_id.0))`.

---

## 4 · Layers panel wiring (`pixors-desktop`)

### 4.1 · `Msg` enum additions

**File:** `pixors-desktop/src/panel/layers.rs`

```rust
#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    Select(LayerId),
    ToggleVisibility(LayerId),
    SetOpacity(LayerId, f32),    // NEW — slider drag
}
```

Switch from index-based `Select(usize)` / `ToggleVisibility(usize)` to `LayerId`
so the controller doesn't need to re-derive IDs from indices.

### 4.2 · Opacity slider in layer row

In `layer_row()`, add a `slider(0.0..=1.0, layer.opacity, |v| Msg::SetOpacity(layer.id, v))`
below the layer name row. Show the percentage as `"{}%", (opacity * 100.0) as u32`.

### 4.3 · Controller wiring

**File:** `pixors-desktop/src/controller.rs`, in `handle_layers_msg()`:

```rust
layers_panel::Msg::Select(id) => {
    if let Some(tab_id) = self.state.active_tab_id() {
        let _ = self.dispatcher.dispatch(Arc::new(SelectLayer { tab: tab_id, layer: id }), &mut self.state);
    }
}
layers_panel::Msg::ToggleVisibility(id) => {
    if let Some(tab) = self.state.active_tab() {
        let visible = tab.layers.iter().find(|l| l.id == id).map(|l| !l.visible).unwrap_or(true);
        let _ = self.dispatcher.dispatch(Arc::new(SetLayerVisibility { tab: tab.id, layer: id, visible }), &mut self.state);
        // re-trigger display pipeline for active tab
        self.queue_display_refresh(tab.id);
    }
}
layers_panel::Msg::SetOpacity(id, opacity) => {
    if let Some(tab) = self.state.active_tab() {
        let prev = tab.layers.iter().find(|l| l.id == id).map(|l| l.opacity).unwrap_or(1.0);
        let _ = self.dispatcher.dispatch(Arc::new(SetLayerOpacity { tab: tab.id, layer: id, opacity, prev_opacity: prev }), &mut self.state);
        self.queue_display_refresh(tab.id);
    }
}
```

`queue_display_refresh(tab_id)` cancels any in-flight background pipeline for that
tab and re-runs `run_mip_fetch` at the current mip/range from the viewport state.

---

## 5 · Filter panel wiring (`pixors-desktop`)

### 5.1 · Replace `filter.rs` with `new_filter.rs`

`new_filter.rs` is the fully designed filter panel UI (already in the codebase at
`pixors-desktop/src/panel/new_filter.rs`). Wire it as the real filter panel:

1. Delete `filter.rs` (the old blur-only panel).
2. Rename `new_filter.rs` → `filter.rs`.
3. Update `pixors-desktop/src/panel/mod.rs` references.

### 5.2 · `Msg` enum for filter panel

```rust
#[derive(Debug, Clone)]
pub enum Msg {
    AddFilter(LayerFilter),          // user picks from add menu
    RemoveFilter(usize),             // trash button on row
    ToggleFilter(usize, bool),       // eye toggle per filter row
    SetBlurRadius(usize, f32),       // slider in expanded blur row
    ResetFilter(usize),              // reset button
    CollapseFilter(usize),
    ExpandFilter(usize),
    Close,
}
```

### 5.3 · `FilterPanelState` (in `App` or per-tab)

Track which filter row is expanded + in-progress slider value (for live preview
without spamming state with every drag tick):

```rust
pub struct FilterPanelState {
    pub expanded: Option<usize>,
    pub dragging_blur: Option<f32>,  // preview value while slider held
}
```

Hold this in `App` (one per session is fine — it only reflects UI state, not model state).

### 5.4 · Controller wiring

```rust
filter_panel::Msg::SetBlurRadius(idx, radius) => {
    // Live preview: update dragging_blur, run preview pipeline
    self.filter_panel.dragging_blur = Some(radius);
    if let (Some(tab), Some(layer_id)) = (self.state.active_tab(), self.state.active_tab().and_then(|t| t.active_layer)) {
        self.run_blur_preview(tab.id, layer_id, idx, radius);
    }
}
filter_panel::Msg::AddFilter(f) => {
    if let Some((tab_id, layer_id)) = self.active_tab_and_layer() {
        let _ = self.dispatcher.dispatch(Arc::new(AddLayerFilter { tab: tab_id, layer: layer_id, filter: f }), &mut self.state);
    }
}
filter_panel::Msg::RemoveFilter(idx) => {
    if let Some((tab_id, layer_id)) = self.active_tab_and_layer() {
        let removed = ...; // read from state before dispatch
        let _ = self.dispatcher.dispatch(Arc::new(RemoveLayerFilter { tab: tab_id, layer: layer_id, index: idx, removed }), &mut self.state);
        self.queue_display_refresh(tab_id);
    }
}
```

### 5.5 · Live blur preview pipeline

`run_blur_preview` in `controller.rs` (rewrite to match new model):

```
CacheReader(active_layer cache_dir, current_mip, visible_range)
→ ColorConvert(ACEScg f16)
→ TileToNeighborhood
→ Blur { radius }
→ ColorConvert(Rgba8 sRGB)
→ TileCacheSink(tab_id, generation=overlay)
```

Then trigger a viewport redraw. On `Msg::CancelPreview` (from the Reset button or
by cancelling), call `dispatcher.cancel_background(tab_id)`, clear the overlay in
`TileCache`, and re-run `run_mip_fetch` (restoring base tiles).

---

## 6 · Export via composite (`pixors-state`)

### 6.1 · Problem today

`Export` action reads from the source file directly:
```
open_image(source_path) → ImageStreamSource → ScanLineToTile → ColorConvert → Encoder
```
This bypasses all layer state (visibility, opacity, filters).

### 6.2 · New export graph

```
[for each visible layer, from disk cache MIP-0]
  CacheReader(layer_i cache_dir, mip=0, range=None)  // full image
  → per-layer filter chain (same as display)
  → [input to Compose]

Compose(opacities, blend_modes)
→ ColorConvert(target_format, target_color_space)
→ Encoder (PNG / TIFF / JPEG)
```

The key difference from the display pipeline: use **MIP-0** (full resolution) and
`range=None` (all tiles).

### 6.3 · `Export` action changes

```rust
// pixors-state/src/action/actions/export.rs
impl Action for Export {
    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        let tab = state.tab(self.tab).ok_or("tab not found")?;
        let visible: Vec<&Layer> = tab.layers.iter().filter(|l| l.visible).collect();

        // build ExecGraph (same multi-branch logic as display §3.2.3, but mip=0, range=None)
        let graph = build_export_graph(tab, &self.config, self.dpi, self.icc_profile.clone())?;

        Ok(PreparedAction::Pipeline { mode: PipelineMode::Apply, graph, routed_tab: None })
    }
}
```

Extract the graph construction into a shared function usable by both the export
action and the desktop display. If the function needs to live in `pixors-state`
(because `Export` is there), keep it there. The desktop's `build_display_graph` in
controller can duplicate the pattern without sharing code.

---

## 7 · Checkerboard transparency in viewport

### 7.1 · Fragment shader

**File:** `pixors-desktop/src/viewport/pipeline.rs` (or the WGSL shader it references)

Before blending the tile texture, generate a checkerboard pattern based on
fragment position:

```wgsl
fn checkerboard(pos: vec2<f32>, cell: f32) -> f32 {
    let c = floor(pos / cell);
    return select(0.85, 0.65, (i32(c.x) + i32(c.y)) % 2 == 0);
}

// In fragment shader, before tile blend:
let check = checkerboard(in.position.xy, 8.0);  // 8px cells
let bg = vec4<f32>(check, check, check, 1.0);

// alpha-over the tile onto bg:
let tile_col = textureSample(tile_texture, tile_sampler, uv);
let out_rgb = tile_col.rgb * tile_col.a + bg.rgb * (1.0 - tile_col.a);
let out = vec4<f32>(out_rgb, 1.0);
```

If the viewport shader is in Slang, add the equivalent there.

### 7.2 · Opt-in flag

Add `show_transparency_grid: bool` to `ViewportState`. Default `true`. Add a
toggle in the View menu for "Show transparency grid". Pass the flag to the pipeline
via a small uniform or as a shader define.

---

## 8 · JPEG decode

### 8.1 · Crate

Add `image = { version = "0.25", default-features = false, features = ["jpeg"] }`
to `pixors-image/Cargo.toml`.

### 8.2 · `JpegDecoder`

**File:** `pixors-image/src/jpeg/mod.rs` (new)

Implement `ImageDecoder` + `PageStream` following the exact pattern of
`pixors-image/src/png/mod.rs`:

- `JpegDecoder::open(path) -> Result<Image, Error>`
- `JpegPageStream::next_scanline() -> Option<ScanLine>`
- Map `image::ColorType` → `PixelFormat` (RGB8 → `PixelFormat::Rgb8`, RGBA8 → `PixelFormat::Rgba8`, L8 → `PixelFormat::Gray8`, LA8 → `PixelFormat::GrayAlpha8`)
- JPEG is always single-page; `PageInfo.frame_count = 1`, `PageInfo.blend_mode = BlendMode::Normal`

### 8.3 · Wire into `open_image()`

**File:** `pixors-image/src/image.rs`

```rust
pub fn open_image(path: &Path) -> Result<Image, Error> {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("png") => PngDecoder::open(path),
        Some("tif") | Some("tiff") => TiffDecoder::open(path),
        Some("jpg") | Some("jpeg") => JpegDecoder::open(path),  // NEW
        _ => Err(Error::UnsupportedFormat),
    }
}
```

### 8.4 · Export to JPEG

Add `EncoderConfig::Jpeg(JpegExportConfig)`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JpegExportConfig {
    pub quality: u8,  // 1–100
}
```

`JpegEncoderStage` (new file `pixors-image/src/sink/jpeg_encoder.rs`):

- Takes Tile stream (Rgba8 sRGB), strips alpha, writes JPEG via `image` crate
- ColorConvert to `Rgb8` happens in the graph before this sink (same pattern as PNG)

Wire into `Export::prepare()` match arm and into the export modal's format list.

### 8.5 · Export modal: add JPEG tab

**File:** `pixors-desktop/src/modal/export/mod.rs`

Add `ExportFormat::Jpeg` variant. Add `jpeg.rs` module alongside `png.rs` / `tiff.rs`
with a quality slider (1–100, default 90). Update the format tab bar in `view.rs`.

---

## 9 · Sequencing

Implement in this order to avoid integration pain:

1. **State model** (§1): `LayerFilter`, remove old `FilterState`, `BlendMode` unify
2. **New actions** (§2): `SetLayerVisibility`, `SetLayerOpacity`, `SelectLayer`
3. **Layer panel wiring** (§4): get visibility toggle and select working first
4. **Per-layer cache dir** (§3.2.4 + OpenFile update): prerequisite for display graph
5. **Display composite graph** (§3.2): single visible layer first, then multi
6. **Checkerboard** (§7): quick win, visible immediately
7. **Filter actions** (§2.4, §2.5) + **filter panel wiring** (§5)
8. **Blur preview pipeline** (§5.5): needs filter actions and display graph working
9. **Export via composite** (§6): needs display graph pattern working
10. **JPEG decode** (§8.1–8.3)
11. **JPEG export** (§8.4–8.5)

---

## 10 · What is explicitly out of scope for Phase 10

- Blend modes beyond Normal/alpha-over
- Multi-layer TIFF files (the decode infrastructure is there, but the compositor UX
  for managing imported layers is Phase 11)
- Layer groups, clipping masks
- Any operation other than Blur in the filter stack
- Undo/redo for filter operations (state mutations happen, but history is not snapshotted)
- WEBP / AVIF decode or encode (Phase 11)
- EXR decode or encode (Phase 11+)

---

## 11 · Files changed (summary for the implementing AI)

### `pixors-engine`
- `src/common/blend.rs` — new, defines `BlendMode`
- `src/lib.rs` — `pub mod common { pub mod blend; }`

### `pixors-image`
- `src/image.rs` — `BlendMode` re-exported from engine, `JpegDecoder` arm in `open_image()`
- `src/jpeg/mod.rs` — new `JpegDecoder` + `JpegPageStream`
- `src/sink/jpeg_encoder.rs` — new `JpegEncoderStage`
- `src/codec.rs` — `EncoderConfig::Jpeg(JpegExportConfig)`, `JpegExportConfig` struct

### `pixors-ops`
- `src/processor/compose.rs` — `opacities: Vec<f32>` field, opacity pre-multiply in blend loop

### `pixors-state`
- `src/tab.rs` — `LayerFilter` enum, `filters: Vec<LayerFilter>` on `Layer`, remove `FilterState` from `Tab`
- `src/action/actions/mod.rs` — add new action modules
- `src/action/actions/set_layer_visibility.rs` — new
- `src/action/actions/set_layer_opacity.rs` — new
- `src/action/actions/select_layer.rs` — new
- `src/action/actions/set_layer_filter.rs` — new
- `src/action/actions/add_layer_filter.rs` — new
- `src/action/actions/remove_layer_filter.rs` — new
- `src/action/actions/open_file.rs` — write to `layer_{id}/` subdir
- `src/action/actions/export.rs` — build composite graph instead of raw file read
- `src/lib.rs` — remove `FilterState` re-export if any

### `pixors-desktop`
- `src/panel/filter.rs` — replace with `new_filter.rs` content, add `FilterPanelState`
- `src/panel/new_filter.rs` — delete (merged into `filter.rs`)
- `src/panel/layers.rs` — `Msg` uses `LayerId`, add opacity slider
- `src/panel/mod.rs` — update any changed re-exports
- `src/controller.rs` — `handle_layers_msg`, `handle_filters_msg`, `build_display_graph()`, `run_mip_fetch` uses new graph, `run_blur_preview` uses layer filter index, `queue_display_refresh()`
- `src/app.rs` — `FilterPanelState` field, remove old `filter` field references
- `src/modal/export/mod.rs` — `ExportFormat::Jpeg`, `Msg::JpegQuality(f32)`
- `src/modal/export/jpeg.rs` — new quality slider view
- `src/modal/export/view.rs` — add JPEG tab to format bar
- `src/viewport/pipeline.rs` or WGSL/Slang shader — checkerboard fragment logic
- `src/viewport/viewport_state.rs` — `show_transparency_grid: bool`
