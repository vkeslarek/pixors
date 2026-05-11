# Document Model Refactor Plan

**Target:** phase 10 and beyond  
**Scope:** `pixors-document` internals + desktop boundary  
**Philosophy:** simple abstractions, dynamic dispatch where it eliminates giant enums, no premature optimisation

---

## 1. Current State (what already exists)

`pixors-document` was introduced in phase 9 and has a good skeleton:

| File | Status |
|---|---|
| `document/mod.rs` — `Document`, `NodeId` | ✅ good |
| `document/canvas.rs` — `CanvasInfo` | ✅ good |
| `document/asset.rs` — `AssetStore`, `AssetId` | ✅ minimal, sufficient for phase 10 |
| `document/develop.rs` — `DevelopState`, `Adjustment` | ✅ placeholder, do not expand yet |
| `document/layer.rs` — `LayerNode`, `LayerFilter`, `BlendSpec` | ✅ good skeleton |
| `session.rs` — `SessionState`, `PreviewState` | ✅ exists, needs minor additions |
| `editor.rs` — `EditorState` | ✅ good |
| `tab.rs` — `Tab` | ✅ exists, **missing `History` field** |
| `history.rs` — `History` (snapshot-based) | ❌ wrong model — replace entirely |
| `action/mod.rs` — `Action`, `Dispatcher` | ✅ good, minor additions needed |

**Gaps (what this plan adds):**
1. `mutation/` module — `DocumentMutation` trait + concrete implementations
2. `view/` module — `DocumentView`, `LayerPanelItem`, `ParamSpec`
3. `LayerFilter::params()` — exposes filter params for generic desktop rendering
4. `History` replacement — mutations-based undo/redo
5. `MutateDocument` action — wraps a mutation as an `Action`
6. Preview/commit actions — `UpdatePreview`, `CommitPreview`, `CancelPreview`
7. Desktop wiring — panels read `DocumentView` instead of `Document` directly

---

## 2. Target Module Layout

```
pixors-document/src/
├── lib.rs                      ← re-exports everything public
├── document/
│   ├── mod.rs                  ← Document, NodeId (keep, small additions)
│   ├── canvas.rs               ← CanvasInfo (unchanged)
│   ├── asset.rs                ← AssetStore, AssetId (unchanged for now)
│   ├── develop.rs              ← DevelopState (unchanged for now)
│   └── layer.rs                ← LayerNode, LayerFilter + params() method
├── mutation/
│   ├── mod.rs                  ← DocumentMutation trait (typetag)
│   └── impls.rs                ← concrete mutations
├── view/
│   ├── mod.rs                  ← DocumentView<'a>
│   ├── layers.rs               ← LayerPanelItem, LayerKind
│   └── params.rs               ← ParamSpec, ParamValue
├── history.rs                  ← History (mutation-based)
├── session.rs                  ← SessionState (keep, minor additions)
├── editor.rs                   ← EditorState (unchanged)
├── tab.rs                      ← Tab (add `history: History` field)
└── action/
    ├── mod.rs                  ← Action trait, Dispatcher (unchanged)
    └── actions/
        ├── open_file.rs        ← (unchanged)
        ├── export.rs           ← (unchanged)
        ├── switch_tab.rs       ← (unchanged)
        ├── close_tab.rs        ← (unchanged)
        ├── undo_redo.rs        ← NEW: UndoAction, RedoAction
        └── preview.rs          ← NEW: UpdatePreview, CommitPreview, CancelPreview
```

---

## 3. The DocumentMutation Trait

### 3.1 Why a trait and not an enum

`LayerFilter` variants (Blur, Exposure, …) will grow to ~15–20 over time. If desktop code pattern-matches mutations directly, every new filter type breaks desktop code. With a trait, new mutations are added by implementing the trait — zero desktop changes.

`typetag` gives automatic `serde::Serialize + Deserialize` for `Box<dyn DocumentMutation>` via a string tag. This is the cleanest Rust solution — no manual enum, no registry boilerplate.

### 3.2 Trait definition

```rust
// pixors-document/src/mutation/mod.rs

/// A reversible, serializable operation on a Document.
///
/// All document mutations go through this trait. The desktop dispatches
/// `MutateDocument` actions. MCP calls mutations by name via typetag.
///
/// Rules:
/// - `apply` must be the exact inverse of `undo` and vice versa.
/// - Neither method touches SessionState, GPU state, or the cache.
/// - Prefer storing the "before" value for undo rather than recomputing it.
#[typetag::serde(tag = "type")]
pub trait DocumentMutation: std::fmt::Debug + Send + Sync {
    fn apply(&self, doc: &mut Document);
    fn undo(&self, doc: &mut Document);
    /// Short human-readable label shown in the undo history panel.
    fn label(&self) -> &str;
}
```

Add to `pixors-document/Cargo.toml`:
```toml
typetag = "0.2"
```

### 3.3 Concrete mutations (implement in `mutation/impls.rs`)

**Layer mutations:**

```rust
/// Rename a layer.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetLayerName {
    pub layer: NodeId,
    pub before: String,
    pub after: String,
}

/// Toggle layer visibility.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetLayerVisible {
    pub layer: NodeId,
    pub before: bool,
    pub after: bool,
}

/// Set layer opacity.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetLayerOpacity {
    pub layer: NodeId,
    pub before: f32,
    pub after: f32,
}

/// Set layer blend mode.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetLayerBlend {
    pub layer: NodeId,
    pub before: BlendMode,
    pub after: BlendMode,
}

/// Add a layer at a given position. Index 0 = bottommost.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddLayer {
    pub at_index: usize,
    pub layer: LayerNode,
}

/// Remove a layer. Stores the full node so undo can restore it.
#[derive(Debug, Serialize, Deserialize)]
pub struct RemoveLayer {
    pub index: usize,        // position before removal, for undo insertion
    pub layer: LayerNode,    // full snapshot for undo
}

/// Reorder two layers by swapping their indices.
#[derive(Debug, Serialize, Deserialize)]
pub struct SwapLayers {
    pub index_a: usize,
    pub index_b: usize,
}
```

**Filter mutations:**

```rust
/// Add a filter to a layer's filter stack at a given position.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddLayerFilter {
    pub layer: NodeId,
    pub at_index: usize,
    pub filter: LayerFilter,
}

/// Remove a filter from a layer.
#[derive(Debug, Serialize, Deserialize)]
pub struct RemoveLayerFilter {
    pub layer: NodeId,
    pub index: usize,
    pub filter: LayerFilter,  // snapshot for undo
}

/// Set a single parameter on a filter.
/// The `param` string is the field name ("radius", "ev", etc.).
#[derive(Debug, Serialize, Deserialize)]
pub struct SetFilterParam {
    pub layer: NodeId,
    pub filter_index: usize,
    pub param: String,
    pub before: ParamValue,
    pub after: ParamValue,
}
```

### 3.4 Implementation pattern for `apply`/`undo`

```rust
#[typetag::serde]
impl DocumentMutation for SetLayerVisible {
    fn apply(&self, doc: &mut Document) {
        if let Some(layer) = doc.find_layer_mut(self.layer) {
            layer.visible = self.after;
        }
    }

    fn undo(&self, doc: &mut Document) {
        if let Some(layer) = doc.find_layer_mut(self.layer) {
            layer.visible = self.before;
        }
    }

    fn label(&self) -> &str {
        if self.after { "Show Layer" } else { "Hide Layer" }
    }
}
```

Every concrete mutation follows this same pattern. `apply` is idempotent-if-repeated only if the underlying data is deterministic — that is the caller's responsibility.

---

## 4. LayerFilter Params (avoid giant enum in desktop)

### 4.1 The problem

`LayerFilter` is an enum in `pixors-document`. Today the desktop needs to render a UI for each filter variant. If we `match` on the enum in desktop widgets, every new filter variant requires desktop changes.

### 4.2 Solution: `params()` on the enum, generic rendering in desktop

Add methods to `LayerFilter` (not a trait — the enum stays simple):

```rust
// pixors-document/src/document/layer.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayerFilter {
    Blur { radius: f32 },
    Exposure { ev: f32 },
    // future: Sharpen, Curves, etc.
}

impl LayerFilter {
    /// Human-readable name.
    pub fn label(&self) -> &str {
        match self {
            LayerFilter::Blur { .. } => "Gaussian Blur",
            LayerFilter::Exposure { .. } => "Exposure",
        }
    }

    /// Ordered list of editable parameters for generic UI rendering.
    /// Desktop renders these without knowing the filter type.
    pub fn params(&self) -> Vec<ParamSpec> {
        match self {
            LayerFilter::Blur { radius } => vec![
                ParamSpec::float("radius", "Radius", *radius, 0.0..=64.0),
            ],
            LayerFilter::Exposure { ev } => vec![
                ParamSpec::float("ev", "EV", *ev, -5.0..=5.0),
            ],
        }
    }

    /// Apply a parameter value by name. Returns false if param not found.
    pub fn set_param(&mut self, name: &str, value: &ParamValue) -> bool {
        match (self, name, value) {
            (LayerFilter::Blur { radius }, "radius", ParamValue::F32(v)) => {
                *radius = *v;
                true
            }
            (LayerFilter::Exposure { ev }, "ev", ParamValue::F32(v)) => {
                *ev = *v;
                true
            }
            _ => false,
        }
    }
}
```

The `match` only lives in `pixors-document`. Desktop code is fully generic:

```rust
// pixors-desktop: render any filter's params without knowing the type
for spec in filter.params() {
    match spec.kind {
        ParamKind::Float { value, range } => {
            // render a slider
        }
        ParamKind::Bool { value } => {
            // render a toggle
        }
    }
}
```

### 4.3 ParamSpec types

```rust
// pixors-document/src/view/params.rs

/// Describes one editable parameter of a filter, ready for generic UI rendering.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// Parameter identifier, matches the string used in SetFilterParam.
    pub name: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    pub kind: ParamKind,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    Float { value: f32, range: std::ops::RangeInclusive<f32> },
    Bool  { value: bool },
    Int   { value: i32, range: std::ops::RangeInclusive<i32> },
}

/// Serializable parameter value, used in SetFilterParam mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamValue {
    F32(f32),
    Bool(bool),
    I32(i32),
}

impl ParamSpec {
    pub fn float(name: &'static str, label: &'static str, value: f32,
                 range: std::ops::RangeInclusive<f32>) -> Self {
        Self { name, label, kind: ParamKind::Float { value, range } }
    }

    pub fn bool(name: &'static str, label: &'static str, value: bool) -> Self {
        Self { name, label, kind: ParamKind::Bool { value } }
    }
}
```

---

## 5. DocumentView — Stable API for Desktop Panels

### 5.1 Why it exists

Desktop widgets must not navigate `Document` internals directly. When Document structure changes (layers become a tree, `LayerNode` gains new fields), widgets break. `DocumentView` is a thin adapter that translates Document internals to flat, widget-ready structs.

Phase 10 implementation: **no caching, compute eagerly on call**. Iced calls `view()` every frame; for documents with < 100 layers this is fast enough. Caching can be added later if profiling shows it as a bottleneck.

### 5.2 Structs

```rust
// pixors-document/src/view/layers.rs

#[derive(Debug, Clone)]
pub struct LayerPanelItem {
    pub id: NodeId,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub kind: LayerKind,
    /// Indentation depth for future group nesting. Always 0 in phase 10.
    pub depth: u8,
    pub filter_count: usize,
    pub has_mask: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Pixel,
    SolidColor,
    // future: Adjustment, Group
}
```

```rust
// pixors-document/src/view/mod.rs

/// Derived, widget-ready view of a Document + SessionState.
/// Computed eagerly — no internal cache in phase 10.
pub struct DocumentView<'a> {
    pub document: &'a Document,
    pub session: &'a SessionState,
}

impl<'a> DocumentView<'a> {
    pub fn new(document: &'a Document, session: &'a SessionState) -> Self {
        Self { document, session }
    }

    /// Flat list of layers, bottom-to-top. Phase 10: always depth=0.
    pub fn layers_panel(&self) -> Vec<LayerPanelItem> {
        self.document.layers.iter().map(|l| LayerPanelItem {
            id: l.id,
            name: l.name.clone(),
            visible: l.visible,
            opacity: l.blend.opacity,
            blend_mode: l.blend.mode,
            kind: match &l.source {
                PixelSource::PrimaryAsset { .. } => LayerKind::Pixel,
                PixelSource::SolidColor { .. }   => LayerKind::SolidColor,
            },
            depth: 0,
            filter_count: l.filters.len(),
            has_mask: l.mask.is_some(),
        }).collect()
    }

    /// Returns the active layer node, if any.
    pub fn active_layer(&self) -> Option<&LayerNode> {
        self.session.active_node
            .and_then(|id| self.document.find_layer(id))
    }

    /// Returns the filter params for the active layer's filter at `index`.
    pub fn active_layer_filter_params(&self, filter_index: usize) -> Option<Vec<ParamSpec>> {
        self.active_layer()
            .and_then(|l| l.filters.get(filter_index))
            .map(|f| f.params())
    }

    pub fn canvas(&self) -> &CanvasInfo {
        &self.document.canvas
    }
}
```

### 5.3 How desktop uses it

```rust
// pixors-desktop/src/panel/layers.rs
pub fn view<'a>(view: &'a DocumentView<'_>, active_id: Option<NodeId>) -> Element<'a, Msg> {
    let items = view.layers_panel();
    // render items — no direct Document access
}
```

The desktop constructs `DocumentView` in `App::view()` from the active tab, then passes it down to panels:

```rust
// pixors-desktop/src/app.rs (inside view())
if let Some(tab) = self.state.active_tab() {
    let doc_view = DocumentView::new(&tab.document, &tab.session);
    // pass doc_view to panels
}
```

---

## 6. History — Mutation-Based Undo/Redo

### 6.1 Replace the snapshot model

The current `History` stores `SnapshotId` references to tile archives on disk. This conflates two concerns: document structure undo and raster pixel undo. Replace entirely.

```rust
// pixors-document/src/history.rs

pub struct History {
    /// Ordered list of applied mutations.
    mutations: Vec<Box<dyn DocumentMutation>>,
    /// Index past the last applied mutation. Undo decrements, redo increments.
    cursor: usize,
}

impl History {
    pub fn new() -> Self {
        Self { mutations: Vec::new(), cursor: 0 }
    }

    /// Apply a new mutation and push it to history.
    /// Truncates any undone future when a new branch is created.
    pub fn push(&mut self, mutation: Box<dyn DocumentMutation>, doc: &mut Document) {
        self.mutations.truncate(self.cursor);
        mutation.apply(doc);
        self.mutations.push(mutation);
        self.cursor = self.mutations.len();
    }

    /// Undo the last applied mutation. Returns its label if one was undone.
    pub fn undo(&mut self, doc: &mut Document) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        let m = &self.mutations[self.cursor];
        m.undo(doc);
        Some(m.label())
    }

    /// Redo the next undone mutation.
    pub fn redo(&mut self, doc: &mut Document) -> Option<&str> {
        if self.cursor == self.mutations.len() {
            return None;
        }
        let m = &self.mutations[self.cursor];
        m.apply(doc);
        self.cursor += 1;
        Some(m.label())
    }

    pub fn can_undo(&self) -> bool { self.cursor > 0 }
    pub fn can_redo(&self) -> bool { self.cursor < self.mutations.len() }

    /// Labels of past mutations (oldest first), for UI display.
    pub fn past_labels(&self) -> impl Iterator<Item = &str> {
        self.mutations[..self.cursor].iter().map(|m| m.label())
    }
}
```

### 6.2 Tab change

```rust
// pixors-document/src/tab.rs
pub struct Tab {
    pub id: TabId,
    pub document: Document,
    pub history: History,      // ← added
    pub session: SessionState,
}
```

`history` is NOT part of `Document` (not serialized to `.pix`). Undo history resets when a file is reopened. This is intentional for phase 10 — full history persistence can come later.

### 6.3 Raster mutations (future paint, crop)

When non-parametric operations are added (paint brush, crop, free transform), they produce pixel data that cannot be reversed by re-applying a parameter change. These operations will implement `DocumentMutation` differently: `apply` and `undo` swap compressed tile snapshots stored alongside `SessionState.cache_dir`. The `DocumentMutation` trait itself does not need to change — only the implementation changes.

A `RasterSnapshot` mutation might look like:
```
apply:  copy after_tiles → active cache, update doc metadata
undo:   copy before_tiles → active cache, update doc metadata
```

The tile data lives in `session.cache_dir`, never in `Document`. `DocumentMutation::apply` only mutates the `Document` struct.

---

## 7. Preview vs Commit

### 7.1 The pattern

```
User drags slider
  → UpdatePreview action (fast, no history)
      → writes to session.active_preview.overrides
      → triggers preview render (low-res, high mip)

User releases slider  
  → CommitPreview action
      → creates SetFilterParam mutation from overrides
      → calls history.push(mutation, &mut doc)
      → clears session.active_preview
      → triggers full-res render

User presses Escape
  → CancelPreview action
      → clears session.active_preview
      → triggers re-render from document (no mutation)
```

### 7.2 PreviewState (already exists, keep as-is)

```rust
// pixors-document/src/session.rs — already exists
pub struct PreviewState {
    pub target_node: NodeId,
    pub overrides: HashMap<String, ParamValue>,
    pub preview_mip: u32,
}
```

`AdjustmentValue` rename: unify to `ParamValue` (the same concept now in `view/params.rs`). Update `session.rs` to `use crate::view::params::ParamValue`.

### 7.3 Preview actions (new file)

```rust
// pixors-document/src/action/actions/preview.rs

/// Update the live preview for one parameter. Does not touch History.
#[derive(Debug)]
pub struct UpdatePreview {
    pub tab: TabId,
    pub target_node: NodeId,
    pub param: String,
    pub value: ParamValue,
    pub preview_mip: u32,
}

impl Action for UpdatePreview {
    fn target_tab(&self) -> Option<TabId> { Some(self.tab) }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab) {
            let preview = tab.session.active_preview.get_or_insert_with(|| PreviewState {
                target_node: self.target_node,
                overrides: HashMap::new(),
                preview_mip: self.preview_mip,
            });
            preview.overrides.insert(self.param.clone(), self.value.clone());
        }
        Ok(PreparedAction::StateOnly)
        // Caller dispatches a separate render request after this returns.
        // Alternatively: return a Pipeline variant here to do both atomically.
    }

    fn apply(&self, _: &mut EditorState, _: PipelineStatus) {}
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool { false }
}

/// Commit the current preview overrides as a real document mutation.
#[derive(Debug)]
pub struct CommitPreview {
    pub tab: TabId,
}

impl Action for CommitPreview {
    fn target_tab(&self) -> Option<TabId> { Some(self.tab) }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab) {
            if let Some(preview) = tab.session.active_preview.take() {
                for (param, value) in &preview.overrides {
                    // Find filter index for preview.target_node
                    if let Some(layer) = tab.document.find_layer(preview.target_node) {
                        // target_node here is a filter's parent layer, and the
                        // filter is identified by a filter_index stored in preview.
                        // See §7.4 for how to track filter_index.
                        let _ = (layer, param, value);
                    }
                    // Produce SetFilterParam mutations and push to history.
                }
            }
        }
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _: &mut EditorState, _: PipelineStatus) {}
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool { false } // history pushed inside prepare
}

/// Discard preview, revert to document state.
#[derive(Debug)]
pub struct CancelPreview {
    pub tab: TabId,
}

impl Action for CancelPreview {
    fn target_tab(&self) -> Option<TabId> { Some(self.tab) }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.session.active_preview = None;
        }
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _: &mut EditorState, _: PipelineStatus) {}
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool { false }
}
```

### 7.4 Extend PreviewState to track filter index

`PreviewState.target_node` currently refers to a `NodeId`. For filters, we also need the index within the layer's filter stack:

```rust
pub struct PreviewState {
    pub layer_id: NodeId,
    pub filter_index: usize,   // ← added
    pub overrides: HashMap<String, ParamValue>,
    pub preview_mip: u32,
}
```

This removes ambiguity: the preview is always for a specific filter on a specific layer.

### 7.5 Render compiler reads preview overrides

When the render compiler builds a pipeline graph from a Tab, it must check `session.active_preview` and substitute overridden parameter values. The compiler is not part of this refactor plan, but the contract is:

```
compile_graph(doc: &Document, session: &SessionState, ...) -> ExecGraph
```

Inside `compile_graph`: if `session.active_preview.is_some()`, override the filter params for the matching layer+filter_index before building the `Blur`/`Exposure`/… stage.

---

## 8. Dual-trait Pattern: Action + DocumentMutation

There is no `MutateDocument` wrapper. Simple document mutations implement **both** `Action` and `DocumentMutation` directly. This eliminates indirection.

### 8.1 Why no wrapper

`MutateDocument { mutation: Box<dyn DocumentMutation> }` is just boilerplate. `SetLayerVisible` can implement `Action` itself with `prepare → StateOnly` and `apply → history.push(Arc::clone(self))`. Same result, one fewer type.

### 8.2 The dual-trait pattern

```rust
// SetLayerVisible implements BOTH traits.

// As DocumentMutation: the reversible unit stored in History.
#[typetag::serde]
impl DocumentMutation for SetLayerVisible {
    fn apply(&self, doc: &mut Document) { ... }
    fn undo(&self, doc: &mut Document) { ... }
    fn label(&self) -> &str { ... }
}

// As Action: how the Dispatcher runs it.
impl Action for SetLayerVisible {
    fn target_tab(&self) -> Option<TabId> { Some(self.tab) }

    fn prepare(&self, _: &mut EditorState) -> Result<PreparedAction, String> {
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, state: &mut EditorState, _: PipelineStatus) {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.history.push(Arc::new(self.clone()) as Arc<dyn DocumentMutation>,
                             &mut tab.document);
            tab.session.redraw_seq += 1;
        }
    }

    fn undo(&self, state: &mut EditorState) {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.history.undo(&mut tab.document);
            tab.session.redraw_seq += 1;
        }
    }

    fn record_in_history(&self) -> bool { false } // history pushed inside apply above
}
```

This requires `SetLayerVisible: Clone + Serialize`. All concrete mutations in `mutation/impls.rs` derive `Clone, Serialize, Deserialize`.

`History` stores `Arc<dyn DocumentMutation>`:

```rust
pub struct History {
    mutations: Vec<Arc<dyn DocumentMutation>>,
    cursor: usize,
}
```

### 8.3 Macro to reduce dual-trait boilerplate (optional)

All simple mutations have identical `Action` impls (StateOnly prepare, history.push in apply, history.undo in undo). A macro can generate this:

```rust
impl_document_action!(SetLayerVisible, tab);
impl_document_action!(SetLayerOpacity, tab);
impl_document_action!(SetFilterParam,  tab);
// etc.
```

Where `impl_document_action!(T, tab_field)` expands to the `impl Action for T` block above. Implement this macro only if the repetition becomes painful — 5–6 types is fine without it.

### 8.4 Desktop usage

```rust
// Toggle layer visibility
self.dispatch(Arc::new(SetLayerVisible {
    tab: tab_id,
    layer: id,
    before: current,
    after: !current,
}));
```

Note `SetLayerVisible` gains a `tab: TabId` field (not present in the DocumentMutation-only signature). The `tab` field is used by `Action::target_tab` and `Action::apply` — it is ignored by `DocumentMutation::apply` and `DocumentMutation::undo` (which receive `&mut Document` directly, no TabId needed).

---

## 9. Action / DocumentMutation Boundary

| Responsibility | Lives in | Examples |
|---|---|---|
| Pipeline construction | `Action::prepare` | OpenFile, Export |
| Document mutation (no pipeline) | dual-trait: `Action` + `DocumentMutation` | SetLayerVisible, SetFilterParam, AddLayer |
| Preview override (live slider) | `UpdatePreview` action (Action only, no mutation) | slider drag |
| Commit preview → history | `CommitPreview` action (creates + pushes mutation) | slider release |
| History undo | `UndoAction` → `History::undo` | Cmd+Z |
| History redo | `RedoAction` → `History::redo` | Cmd+Shift+Z |

### Pipeline actions that also mutate document

`OpenFile` constructs a `Tab` and pushes it to `EditorState`. This is structural state, not a `Document` mutation — it stays in `Action::apply`. No change needed.

`Export` reads the document, runs a pipeline, writes a file. No document mutation. No change needed.

A future `ApplyBlur` (destructive flatten) would: run pipeline → receive tiles → create a `RasterSnapshot` mutation → call `history.push`. That mutation lives in the `apply` callback, not in `DocumentMutation::apply` directly.

---

## 10. Desktop Changes

### 10.1 panels/layers.rs

**Current:** `pub fn view<'a>(layers: &'a [LayerNode], active_idx: usize)`  
**After:** `pub fn view<'a>(view: &'a DocumentView<'_>, active_id: Option<NodeId>)`

Messages change from index-based to NodeId-based:

```rust
pub enum Msg {
    Select(NodeId),
    ToggleVisibility(NodeId),
    SetOpacity(NodeId, f32),
}
```

Controller dispatches the mutation directly as an `Action`:
```rust
layers_panel::Msg::ToggleVisibility(id) => {
    let tab = self.state.active_tab().unwrap();
    let current = tab.document.find_layer(id).unwrap().visible;
    self.dispatch(Arc::new(SetLayerVisible {
        tab: tab.id, layer: id, before: current, after: !current,
    }));
}
```

### 10.2 panel/new_filter.rs

Replace hardcoded `Vec<FilterNode>` with document-derived data.

`State` becomes:
```rust
pub struct State {
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
    // remove: pub filters: Vec<FilterNode>
    // filters come from DocumentView::active_layer_filters()
}
```

`State::view` takes `Option<&[LayerFilter]>` (from `DocumentView::active_layer()?.filters`).

Each filter renders its params via `filter.params()` — no match on filter type in desktop code.

Messages:
```rust
pub enum Msg {
    OpenFilterSearch,
    ToggleExpand(usize),
    SetParam { filter_index: usize, param: String, value: ParamValue },
    BeginPreview { filter_index: usize, param: String, value: ParamValue },
    CommitPreview,
    CancelPreview,
    RemoveFilter(usize),
    DragStart(usize),
    DragHover(usize),
    DragDrop,
}
```

Controller dispatches `SetFilterParam { .. }` (implements both Action + DocumentMutation) and `UpdatePreview { .. }` directly.

### 10.3 panel/filter.rs

`panel/filter.rs` is a legacy wrapper — will be replaced by `new_filter.rs` in phase 10. Remove `SetBlur(f32)` message. The blur slider moves to `new_filter.rs` via `Msg::BeginPreview`.

### 10.4 controller.rs

Add routing for new panel messages. No business logic — just translation to `Action` or `MutateDocument`. Example:

```rust
Msg::NewFilterPanel(new_filter::Msg::SetParam { filter_index, param, value }) => {
    if let Some(tab) = self.state.active_tab() {
        let layer_id = tab.session.active_node.unwrap();
        if let Some(filter) = tab.document.find_layer(layer_id)
            .and_then(|l| l.filters.get(filter_index))
        {
            let before = filter.params()
                .iter()
                .find(|p| p.name == param)
                .map(|p| match &p.kind {
                    ParamKind::Float { value, .. } => ParamValue::F32(*value),
                    _ => todo!(),
                })
                .unwrap();
            let tab_id = tab.id;
            self.dispatch(Arc::new(SetFilterParam {
                tab: tab_id, layer: layer_id,
                filter_index, param, before, after: value,
            }));
        }
    }
}
```

---

## 11. Data Flow Diagram

```
User event (slider drag)
    │
    ▼
pixors-desktop: Msg::NewFilterPanel(BeginPreview { filter_index, param, value })
    │
    ▼
controller.rs: dispatch(UpdatePreview { tab, layer_id, filter_index, param, value })
    │
    ▼
pixors-document: Action::prepare → mutates session.active_preview.overrides
    │
    ▼
controller.rs: dispatch(render request)   ← separate, after UpdatePreview returns
    │
    ▼
pixors-document: compile_graph reads session.active_preview → substitutes params
    │
    ▼
Pipeline runs → tiles → TileCache → redraw

User releases slider
    │
    ▼
pixors-desktop: Msg::NewFilterPanel(CommitPreview)
    │
    ▼
controller.rs: dispatch(CommitPreview { tab })
    │
    ▼
pixors-document: CommitPreview::prepare →
    takes session.active_preview →
    creates SetFilterParam mutation →
    history.push(mutation, &mut doc) →
    clears session.active_preview
    │
    ▼
controller.rs: dispatch(full render request)
```

```
Tab
├── document: Document          ← serialized to .pix
│   ├── canvas: CanvasInfo
│   ├── assets: AssetStore
│   ├── develop: DevelopState   ← phase 10: empty, slot only
│   └── layers: Vec<LayerNode>
│       └── LayerNode
│           ├── id: NodeId
│           ├── blend: BlendSpec
│           ├── source: PixelSource
│           ├── filters: Vec<LayerFilter>
│           └── mask: Option<Mask>
│
├── history: History            ← NOT serialized
│   ├── mutations: Vec<Arc<dyn DocumentMutation>>
│   └── cursor: usize
│
└── session: SessionState       ← NOT serialized
    ├── cache_dir: PathBuf
    ├── redraw_seq: u64
    ├── active_node: Option<NodeId>
    ├── active_preview: Option<PreviewState>
    ├── view: TabView
    └── pipeline_running: bool
```

---

## 12. Migration Steps

Implement in this order. Each step is independently compilable. PR per step recommended.

### Step 1 — Add `typetag` dependency
- `pixors-document/Cargo.toml`: add `typetag = "0.2"`
- No code changes yet.

### Step 2 — Add `view/params.rs`
- Create `pixors-document/src/view/params.rs` with `ParamSpec`, `ParamKind`, `ParamValue`.
- Create `pixors-document/src/view/mod.rs` with `DocumentView<'a>` struct, empty impl for now.
- Export from `lib.rs`.
- Unify `AdjustmentValue` in `session.rs` → replace with `use crate::view::params::ParamValue`.

### Step 3 — Add `params()` to `LayerFilter`
- Add `label()`, `params()`, `set_param()` methods to `LayerFilter` enum.
- No changes to existing fields or variants.

### Step 4 — Add `mutation/` module
- Create `pixors-document/src/mutation/mod.rs` with `DocumentMutation` trait (typetag).
- Create `pixors-document/src/mutation/impls.rs` with all concrete mutations from §3.3.
- Export from `lib.rs`.

### Step 5 — Replace `History`
- Rewrite `pixors-document/src/history.rs` using `Vec<Arc<dyn DocumentMutation>>` + cursor.
- Add `history: History` to `Tab` struct. Remove it from `SessionState` if present.
- Compile-fix anything that referenced the old `HistoryEntry`/`SnapshotId` types (delete them).

### Step 6 — Implement dual-trait on concrete mutations
- Each mutation in `mutation/impls.rs` also implements `Action` (derive via macro or manually).
- Add `tab: TabId` field to each mutation struct (used by `Action::target_tab`).
- Add `UndoAction` and `RedoAction` in `action/actions/undo_redo.rs`.

### Step 7 — Add preview actions
- Create `pixors-document/src/action/actions/preview.rs` with `UpdatePreview`, `CommitPreview`, `CancelPreview`.
- Extend `PreviewState` in `session.rs` with `filter_index: usize`.

### Step 8 — Complete `DocumentView`
- Implement `DocumentView::layers_panel()`, `active_layer()`, `active_layer_filter_params()`, `canvas()`.
- Create `view/layers.rs` with `LayerPanelItem`, `LayerKind`.

### Step 9 — Wire layers panel to DocumentView
- Update `panel/layers.rs` signature to take `DocumentView`.
- Update `controller.rs` routing for layer messages → `MutateDocument` actions.
- Remove old `active_idx: usize` approach; use `NodeId` throughout.

### Step 10 — Wire filter panel to DocumentView
- Update `panel/new_filter.rs` `State::view` to accept `Option<&[LayerFilter]>`.
- Replace hardcoded `FilterNode` list with document-derived data.
- Wire `BeginPreview`/`CommitPreview`/`CancelPreview` messages to corresponding actions.

### Step 11 — Wire render compiler to PreviewState
- In `controller.rs`, when building the render graph, pass `session.active_preview` to the graph builder.
- The graph builder substitutes overridden param values before constructing `Blur`/etc. stages.

---

## 13. What Is Out of Scope for This Refactor

| Feature | Notes |
|---|---|
| DevelopState expansion | Slot exists. Do not add adjustments until layer pipeline is stable. |
| AssetStore multi-asset (placed images) | Field reserved. Phase 11+. |
| Layer groups / tree structure | `Vec<LayerNode>` is flat. Group support requires `enum LayerNode { … }`. Phase 11+. |
| Adjustment layers | Phase 11+. |
| Blend modes (Multiply, Screen, …) | Phase 10 ships Normal/alpha-over only. Enum exists, compose only uses Normal. |
| Raster mutations (paint, crop) | Phase 11+. History trait is ready; tile snapshot mechanism is not. |
| Undo/redo keyboard shortcuts | Wire Cmd+Z → `History::undo` after step 5. Simple addition. |
| History panel UI | Out of scope for phase 10. `History::past_labels()` is ready when needed. |
| `.pix` file format save/load | Document is serde-ready. Save/load action not implemented yet. |
| MCP DocumentMutation invocation | typetag is in place; MCP server wiring is a separate task. |
| DocumentEvent bus | `redraw_seq` increment is sufficient for phase 10. |
| DocumentView caching / invalidation | Compute eagerly in phase 10. Profile before adding caching. |

---

## 14. Open Questions

1. **`CommitPreview` implementation detail**: `CommitPreview::prepare` needs to produce one `SetFilterParam` mutation per override. Currently `PreviewState.overrides` is `HashMap<String, ParamValue>` which loses ordering. If multiple params can be overridden simultaneously (future: multi-param filters), consider `Vec<(String, ParamValue)>` instead. For phase 10 (single-param preview only), HashMap is fine.

2. **`SetFilterParam` before-value**: `SetFilterParam` stores `before: ParamValue` for undo. When `CommitPreview` runs, it reads the before-value from the document (not from the preview override). Ensure the document hasn't been mutated between preview start and commit (it shouldn't be, since preview is live-edit only).

3. **Preview render quality**: `PreviewState.preview_mip` defaults to what? Phase 10 suggestion: use mip 1 (half resolution) for sliders, mip 0 for commit. This should be a constant or come from a viewport state hint.

4. **Dispatcher undo/redo**: `Action::undo` currently calls `history.undo()` inside `MutateDocument`. But `Dispatcher::dispatch` also calls `action.undo()` for the Action undo/redo mechanism. Clarify: is history undo driven by `Dispatcher` or by a dedicated `UndoAction`? Recommendation: add `UndoAction` and `RedoAction` that simply call `tab.history.undo/redo`. This is cleaner than the current `Action::undo` pattern for structural undo.

5. **`new_filter.rs` vs `filter.rs`**: Both panels partially overlap. Phase 10 decision: retire `filter.rs`, promote `new_filter.rs` to `filter.rs`. Or keep both and route the blur slider exclusively through `new_filter.rs`. Make this call before step 10.
