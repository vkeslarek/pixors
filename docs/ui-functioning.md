# UI Functioning — Editor State, Tabs, Actions

## Context

Today, `pixors-desktop` is a **single-image flat state container**: `App` directly holds `image_path`, `image_dims`, `cache_dir`, one `ViewportCache`. The `tab_bar` component shows decorative tabs (`Vec<String>`) but no tab is bound to a real file. Sinks (`ViewportCacheSink`, `TileSink`) use a `OnceLock<Callback>` global registration — a single setter for the entire app lifetime, which structurally prevents per-tab caches.

`pixors-executor` is intentionally **stateless**: it runs DAGs of stages (Source → Transform → Op → Sink) and mutates external state via sinks. The desktop layer owns all editor state. To get multi-tab editing, undo/redo, layer chains, multipage files, we need an explicit state-and-action architecture in the desktop.

## Goals

1. **Multi-tab editing**: each tab is a real `Tab` with its own file, descriptor, layers, viewport cache, edit chain.
2. **Explicit state**: `EditorState` owns all editor data; `App` becomes a thin wrapper around it plus UI ephemeral state.
3. **Action pattern**: every user intent (open file, apply filter, export, undo) is an `Action`. An action is `prepare → apply → undo`: `prepare` builds a pipeline from current state, `apply` commits the pipeline result into state, `undo` reverts state from a snapshot in the history cache.
4. **Per-tab sink routing**: sinks accept a `TabId` so the same sink kind can write to different `ViewportCache` instances depending on which tab the pipeline belongs to.
5. **Metadata-first opens**: `Image::open` (cheap, sync) populates the new `Tab` immediately — title, dimensions, color space, ICC, dpi, page count, auto-created base layer. Pixels stream in afterwards via the pipeline.
6. **Pipeline mode (Background vs Apply)**: actions declare whether their pipeline runs in the background (UI live) or in apply mode (UI locked, progress bar shown). Apply mode is atomic — failure rolls back via history cache.

## Architecture overview

```
┌─────────────────── App (iced) ──────────────────────────────────┐
│  EditorState                                                    │
│  ├── tabs: Vec<Tab>                                             │
│  │     └── per tab: viewport_cache, layers, chain, history      │
│  ├── active: Option<TabId>                                      │
│  ├── pipeline_lock: Option<TabId>  ← set while Apply runs       │
│  └── settings: AppSettings                                      │
│                                                                 │
│  UI state (ephemeral, not part of EditorState):                 │
│  ├── panes, tools, workspace_bar, filters_panel, ...            │
│  ├── show_export_dialog, errors                                 │
│  ├── apply_progress: Option<ApplyProgress>  ← drives loading bar│
│  └── mip_fetch_signal: Arc<Mutex<Vec<(TabId, mip, range)>>>     │
│                                                                 │
│  update(Msg) ─┬─► Msg::Action(Action)                           │
│               │     ├─ Action::prepare(state) → PipelineSpec    │
│               │     ├─ spawn pipeline (background or apply)     │
│               │     └─ on Done: Action::apply(status, &mut s)   │
│               │        on Err & Apply mode: roll back from hist │
│               ├─► UI Msg::* → in-place ephemeral mutation       │
│               └─► Msg::PipelineEvent → route to tab progress    │
└─────────────────────────────────────────────────────────────────┘
                       │
                       │ Action::prepare builds graph
                       ▼
              ┌─────────────────────┐
              │ pixors-executor     │
              │ Pipeline (DAG)      │
              │ Source → Op → Sink  │
              └──────┬──────────────┘
                     │ sinks write tiles
                     ▼
              ┌─────────────────────────────────────┐
              │ Sink router (keyed by TabId)        │
              │ HashMap<TabId, Arc<Mutex<Cache>>>   │
              │   → committed to right tab's cache  │
              └─────────────────────────────────────┘
                     │
                     ▼
              Tab.viewport_cache (Arc<Mutex<>>)
                     │
                     ▼
              ViewportProgram.draw() reads active tab's cache
```

## State structures

New module: `pixors-desktop/src/state/`.

### `state/tab.rs`

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use pixors_executor::common::image::ImageDescriptor;
use crate::viewport::tile_cache::ViewportCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub struct Tab {
    pub id: TabId,
    pub title: String,                          // displayed in tab bar
    pub source: TabSource,                      // file or new
    pub desc: ImageDescriptor,                  // metadata kept around
    pub cache_dir: PathBuf,                     // pixors_cache for this tab
    pub viewport_cache: Arc<Mutex<ViewportCache>>,
    pub layers: Vec<Layer>,
    pub active_layer: Option<LayerId>,
    pub chain: EditChain,
    pub history: History,
    pub view: TabView,                          // per-tab camera state (zoom, pan)
}

pub enum TabSource {
    /// Tab backed by an opened file. The file may have multiple pages
    /// (animation frames, multipage TIFF). `Image::open()` gives us the
    /// metadata; `Image::open_page(idx)` is what individual layers will
    /// reference to stream pixels (see `LayerSource::FilePage`).
    File { path: PathBuf, page_count: usize },
    /// Brand-new tab. The canvas is transparent; the user-visible "fill
    /// color" lives on the auto-created first layer, not on the tab itself.
    NewBlank { width: u32, height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    pub source: LayerSource,
}

/// A layer's pixel source. Mirrors what `pixors-executor` decoders expose:
/// `Image::open_page(page: usize) -> Box<dyn PageStream>` (see
/// `pixors-executor/src/common/image/mod.rs`). For file-backed tabs, a
/// layer references **one specific page** of the parent file's stream.
pub enum LayerSource {
    /// Stream from one page of the tab's source file.
    /// Maps directly to `tab.source.path` + `Image::open_page(page)`.
    FilePage { page: usize },
    /// Solid-color fill (used as the auto-created first layer of a NewBlank tab).
    SolidColor { color: [u8; 4] },
    /// Future: adjustment layer, smart layer (links to another tab), etc.
}

pub struct TabView {
    pub zoom: f32,
    pub pan: (f32, f32),
    pub active_mip: u32,
}

// EditChain + History defined in state/history.rs.
pub struct EditChain { pub ops: Vec<()> }       // expanded later

#[derive(Debug, Clone, Copy)]
pub enum BlendMode { Normal, Multiply /* ... */ }
```

### `state/history.rs`

History keeps **snapshots of state regions touched by Apply-mode actions** so a failure can roll back. It is *not* an action log alone — failed Apply tainted state must be reconstructable from a cache, not by replaying actions backward (which would require every action to be deterministically invertible from current state, hard for filters).

```rust
use crate::state::{TabId, LayerId};

pub struct History {
    pub past:    Vec<HistoryEntry>,             // committed actions (undo stack)
    pub future:  Vec<HistoryEntry>,             // redoable
    pub cache:   HistoryCache,                  // snapshots referenced by entries
}

pub struct HistoryEntry {
    pub action_label: String,                   // "Gaussian Blur 3px"
    pub snapshot_id: SnapshotId,                // points into HistoryCache
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub u64);

pub struct HistoryCache {
    /// Snapshots are ranges of disk tiles + state diffs captured before an
    /// Apply ran. On rollback or undo, we restore from here.
    snapshots: HashMap<SnapshotId, Snapshot>,
    next_id: u64,
}

pub struct Snapshot {
    pub layer_states: Vec<LayerSnapshot>,       // affected layers' metadata
    pub tile_archive: PathBuf,                  // disk dir holding pre-action tiles
}

pub struct LayerSnapshot {
    pub layer: LayerId,
    pub source: super::LayerSource,
    pub visible: bool,
    pub opacity: f32,
    pub blend: super::BlendMode,
}
```

The HistoryCache is what makes Apply mode safe: prepare() captures pre-state (snapshot id), the action runs, and `apply(Ok)` advances `past`. `apply(Err)` or `undo()` discards the partial pipeline output and restores from `cache.snapshots[id]`.

### `state/editor.rs`

```rust
pub struct EditorState {
    tabs: Vec<Tab>,
    active: Option<TabId>,
    next_tab_id: u64,
    next_layer_id: u64,
    /// Set to Some(tab) while an Apply-mode pipeline is running on that tab.
    /// While set, the UI shows the loading bar and **rejects new actions**
    /// targeting the same tab (or globally — see "Pipeline modes" below).
    pub pipeline_lock: Option<TabId>,
}

impl EditorState {
    pub fn new() -> Self { ... }
    pub fn alloc_tab_id(&mut self) -> TabId { ... }
    pub fn alloc_layer_id(&mut self) -> LayerId { ... }

    pub fn push_tab(&mut self, tab: Tab) {
        let id = tab.id;
        self.tabs.push(tab);
        self.active = Some(id);
    }

    pub fn close(&mut self, id: TabId) { ... }
    pub fn switch(&mut self, id: TabId) { ... }

    pub fn active_tab(&self) -> Option<&Tab> { ... }
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> { ... }
    pub fn tab(&self, id: TabId) -> Option<&Tab> { ... }
    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> { ... }

    pub fn is_locked(&self) -> bool { self.pipeline_lock.is_some() }
}
```

### `state/mod.rs`

```rust
pub mod tab;
pub mod editor;
pub mod history;

pub use tab::{Tab, TabId, TabSource, Layer, LayerId, LayerSource, TabView, BlendMode, EditChain};
pub use history::{History, HistoryEntry, HistoryCache, Snapshot, SnapshotId};
pub use editor::EditorState;
```

## Pipeline modes

Two distinct execution modes, picked per Action:

| Mode | UI behavior | Use cases | On error |
|---|---|---|---|
| **Background** | UI stays live. Tile arrival animates the viewport. No global progress bar. | Initial file load, MIP prefetch, cache warmup | toast notification; tab keeps whatever tiles arrived |
| **Apply** | UI locked (`EditorState.pipeline_lock = Some(tab)`). Loading bar shown. New actions targeting the locked tab are rejected. | Filter commit, color-space convert, export, "flatten layers" | rollback from `HistoryCache` snapshot — partial output discarded |

Concretely: while `pipeline_lock = Some(tab)`, the App's `update()` checks before dispatching any new mutating Action against that tab and bounces it (toast: "Pipeline running, please wait"). Read-only UI changes (panel toggles, tab switch) keep working.

How the loading bar works today (`App.loading + progress`) generalizes to one progress slot per Apply pipeline. Multiple tabs can each have their own background pipelines running without locking anything; only Apply mode flips the lock.

## Action: prepare / apply / undo

Every action implements three steps. `prepare` is the only one that builds + spawns a pipeline; `apply` is invoked once the pipeline finishes (called from the App's update on `Msg::PipelineEvent::Done`); `undo` walks back from the history cache.

```rust
use pixors_executor::runtime::event::PipelineEvent;
use pixors_executor::runtime::pipeline::Pipeline;
use crate::state::{EditorState, SnapshotId};

#[derive(Debug, Clone, Copy)]
pub enum PipelineMode { Background, Apply }

/// What `prepare` returns. The pipeline is wrapped because it may have been
/// run synchronously already (e.g. simple state-only actions like
/// `SwitchTab` produce `PreparedAction::StateOnly`).
pub enum PreparedAction {
    /// Action only mutates state, no pipeline needed (e.g. SwitchTab).
    StateOnly,
    /// Action runs a pipeline. Apply is called when it completes.
    Pipeline {
        mode: PipelineMode,
        pipeline: Pipeline,
        /// Snapshot taken before prepare ran (for Apply rollback).
        snapshot: Option<SnapshotId>,
    },
}

pub enum PipelineStatus {
    Done,
    Error(String),
    /// Background-mode pipelines may report partial progress; Apply
    /// only ever resolves to Done or Error.
    Cancelled,
}

pub trait Action: std::fmt::Debug + Send + 'static {
    /// Snapshot any state the action will mutate (Apply mode), build the
    /// pipeline graph, and apply optimistic state changes that should be
    /// visible during execution (e.g. add a placeholder layer, add the
    /// new tab). Returns the prepared work + snapshot id for rollback.
    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String>;

    /// Called when the prepared pipeline completes. Status determines
    /// whether to commit the optimistic changes or trigger a rollback.
    /// For `StateOnly` actions, called immediately by the dispatcher
    /// with `PipelineStatus::Done`.
    fn apply(&self, state: &mut EditorState, status: PipelineStatus);

    /// Restore state from the action's snapshot (or its inverse if the
    /// action is naturally invertible like SwitchTab). Called by undo
    /// stack walks and by Apply-mode rollback on error.
    fn undo(&self, state: &mut EditorState);

    /// What lifecycle history bucket this action belongs to. Most actions
    /// push onto `history.past`; some (SwitchTab, RequestMipFetch) are
    /// transient and never recorded.
    fn record_in_history(&self) -> bool { true }
}
```

### Dispatcher

```rust
pub struct Dispatcher<'a> {
    pub event_tx: &'a tokio::sync::broadcast::Sender<PipelineEvent>,
}

impl<'a> Dispatcher<'a> {
    pub fn dispatch(&self, action: Box<dyn Action>, state: &mut EditorState) -> Result<(), String> {
        match action.prepare(state)? {
            PreparedAction::StateOnly => {
                action.apply(state, PipelineStatus::Done);
                if action.record_in_history() { /* push HistoryEntry */ }
                Ok(())
            }
            PreparedAction::Pipeline { mode, pipeline, snapshot } => {
                if matches!(mode, PipelineMode::Apply) {
                    // Lock the active tab. UI shows progress bar.
                    if let Some(tab) = state.active().map(|t| t.id) {
                        state.pipeline_lock = Some(tab);
                    }
                }
                let event_tx = self.event_tx.clone();
                let action_box = SyncWrap(action);  // shared into apply later
                std::thread::spawn(move || {
                    let result = pipeline.run(None);
                    let _ = event_tx.send(match result {
                        Ok(_)  => PipelineEvent::Done,
                        Err(e) => PipelineEvent::Error(e.to_string()),
                    });
                    // The App receives PipelineEvent::Done, looks up the
                    // pending action by some token, and calls action.apply().
                });
                let _ = snapshot;
                Ok(())
            }
        }
    }
}
```

The dispatcher needs to remember which `Action` is in flight so it can call `apply` when the pipeline finishes. A pending-actions map keyed by a request id, or a single-slot `Option<Box<dyn Action>>` per tab if we forbid concurrent Apply on one tab, is enough. Background actions can be fire-and-forget (their apply is trivial — usually a no-op since sinks already wrote to state).

### Action examples

#### `OpenFile` — Background mode

```rust
pub struct OpenFile {
    pub path: PathBuf,
    // populated during prepare so apply has access:
    pending_tab_id: std::cell::Cell<Option<TabId>>,
}

impl Action for OpenFile {
    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        // 1. Cheap meta read — populates Tab immediately, BEFORE pipeline runs.
        let img = Image::open(&self.path).map_err(|e| e.to_string())?;
        let desc = img.desc.clone();
        let page_count = img.page_count();
        let cache_dir = self.path.with_extension("pixors_cache");

        let viewport_cache = ViewportCache::new();
        viewport_cache.lock().unwrap().signal_new_img(desc.width, desc.height);

        // 2. Build Tab + auto layer from page 0.
        let tab_id = state.alloc_tab_id();
        let layer_id = state.alloc_layer_id();
        let title = self.path.file_name().unwrap_or_default().to_string_lossy().into_owned();

        state.push_tab(Tab {
            id: tab_id,
            title,
            source: TabSource::File { path: self.path.clone(), page_count },
            desc,
            cache_dir: cache_dir.clone(),
            viewport_cache: viewport_cache.clone(),
            layers: vec![Layer {
                id: layer_id, name: "Background".into(), visible: true, opacity: 1.0,
                blend: BlendMode::Normal,
                source: LayerSource::FilePage { page: 0 },
            }],
            active_layer: Some(layer_id),
            chain: EditChain::default(),
            history: History::default(),
            view: TabView { zoom: 1.0, pan: (0.0, 0.0), active_mip: 0 },
        });
        self.pending_tab_id.set(Some(tab_id));

        // 3. Register this tab's viewport cache so sinks can route to it.
        crate::sinks::router::register_tab_cache(tab_id, viewport_cache);

        // 4. Build pipeline — sinks parameterized with tab_id (routing_key).
        let stream = Arc::new(Mutex::new(Some(img.open_page(0).map_err(|e| e.to_string())?)));
        let pipeline = build_open_pipeline(stream, tab_id, cache_dir, &state.tab(tab_id).unwrap().desc)?;

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            pipeline,
            snapshot: None,                     // Background never rolls back
        })
    }

    fn apply(&self, _state: &mut EditorState, status: PipelineStatus) {
        // Tiles already streamed in via sinks; nothing more to commit.
        // Just log / toast on Error.
        if let PipelineStatus::Error(e) = status {
            tracing::error!("OpenFile failed: {e}");
        }
    }

    fn undo(&self, state: &mut EditorState) {
        // Closing the tab + unregistering the cache is the inverse.
        if let Some(id) = self.pending_tab_id.get() {
            crate::sinks::router::unregister_tab_cache(id);
            state.close(id);
        }
    }

    fn record_in_history(&self) -> bool { false }   // OpenFile not an undoable edit
}
```

#### `ApplyFilter` — Apply mode

```rust
pub struct ApplyFilter {
    pub tab: TabId,
    pub layer: LayerId,
    pub params: FilterParams,
    snapshot_id: std::cell::Cell<Option<SnapshotId>>,
}

impl Action for ApplyFilter {
    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        // 1. Snapshot pre-state into HistoryCache.
        let tab = state.tab_mut(self.tab).ok_or("tab not found")?;
        let snap = tab.history.cache.snapshot_layer(self.layer, &tab.cache_dir);
        self.snapshot_id.set(Some(snap));

        // 2. Build apply-mode pipeline (e.g. blur the layer's tiles).
        let pipeline = build_filter_pipeline(self.tab, self.layer, &self.params, tab)?;

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Apply,
            pipeline,
            snapshot: Some(snap),
        })
    }

    fn apply(&self, state: &mut EditorState, status: PipelineStatus) {
        match status {
            PipelineStatus::Done => {
                // Pipeline wrote new tiles to disk + viewport cache.
                // Push HistoryEntry referencing the snapshot.
                if let (Some(tab), Some(snap)) = (state.tab_mut(self.tab), self.snapshot_id.get()) {
                    tab.history.past.push(HistoryEntry { action_label: "Filter".into(), snapshot_id: snap });
                    tab.history.future.clear();
                }
            }
            PipelineStatus::Error(_) | PipelineStatus::Cancelled => {
                // Roll back: restore tiles + layer state from the snapshot.
                self.undo(state);
            }
        }
        state.pipeline_lock = None;             // unlock UI either way
    }

    fn undo(&self, state: &mut EditorState) {
        if let (Some(tab), Some(snap)) = (state.tab_mut(self.tab), self.snapshot_id.get()) {
            tab.history.cache.restore(snap, tab);
            tab.viewport_cache.lock().unwrap().clear_all();
            // Re-emit dirty tiles from the restored archive into the cache.
        }
    }
}
```

### Action variants (initial set)

```rust
// Tabs (mostly StateOnly)
struct OpenFile { path: PathBuf, ... }                    // Background
struct OpenFileDialog;                                    // unwraps to OpenFile
struct NewBlankTab { width: u32, height: u32 };          // StateOnly + Background
struct CloseTab(TabId);                                  // StateOnly
struct SwitchTab(TabId);                                 // StateOnly
struct ReorderTabs { from: usize, to: usize };           // StateOnly

// Layers (mostly Apply once edits exist)
struct AddLayer { tab: TabId, source: LayerSource };     // StateOnly
struct RemoveLayer { tab: TabId, layer: LayerId };       // StateOnly + history
struct SetLayerVisibility { tab, layer, visible };       // StateOnly + history
struct ApplyFilter { tab, layer, params };               // Apply

// Viewport (transient, never recorded)
struct RequestMipFetch { tab, mip, range };              // Background

// Export
struct Export { tab: TabId, path: PathBuf, config };     // Apply (locks UI)

// History
struct Undo(TabId);                                      // StateOnly (calls action.undo())
struct Redo(TabId);                                      // re-run action.prepare/apply
```

## Per-tab sink routing

**Today**: `OnceLock<Arc<CacheCommitFn>>` set once globally.

**Refactor**: replace OnceLock with a router keyed by `TabId`. The sink struct itself carries the routing key.

### Executor side: `pixors-executor/src/sink/viewport_cache_sink.rs`

```rust
pub type CacheCommitFn = Box<dyn Fn(/* routing_key */ u64, u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;
//                                  ^^^^^^^^^^^^^^^^^ added

static CACHE_SINK: OnceLock<Arc<CacheCommitFn>> = OnceLock::new();
// install_viewport_cache_sink keeps its signature; the closure receives routing_key.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportCacheSink {
    pub routing_key: u64,                       // = TabId.0
}
```

The Consumer's `consume()` calls `(self.cb)(self.routing_key, ..., data)`.

`TileSink` gets the same treatment.

### Desktop side: `pixors-desktop/src/sinks/router.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use crate::state::TabId;
use crate::viewport::tile_cache::ViewportCache;

static TAB_CACHES: once_cell::sync::Lazy<RwLock<HashMap<u64, Arc<Mutex<ViewportCache>>>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

pub fn register_tab_cache(id: TabId, cache: Arc<Mutex<ViewportCache>>) {
    TAB_CACHES.write().unwrap().insert(id.0, cache);
}

pub fn unregister_tab_cache(id: TabId) {
    TAB_CACHES.write().unwrap().remove(&id.0);
}

pub fn get_tab_cache(id: u64) -> Option<Arc<Mutex<ViewportCache>>> {
    TAB_CACHES.read().unwrap().get(&id).cloned()
}

/// Called once at app startup.
pub fn install() {
    pixors_executor::sink::viewport_cache_sink::install_viewport_cache_sink(Box::new(
        |routing_key, mip, tx, ty, px, py, w, h, bytes| {
            if let Some(cache) = get_tab_cache(routing_key)
                && let Ok(mut g) = cache.lock()
            {
                use pixors_executor::data::tile::TileGridPos;
                use crate::viewport::tile_cache::CachedTile;
                g.insert(
                    TileGridPos { mip_level: mip, tx, ty },
                    CachedTile { px, py, width: w, height: h, bytes: bytes.to_vec() },
                );
            }
        },
    ));
}
```

## Code movement plan

| Source (today) | Target | Notes |
|---|---|---|
| `app.rs::App` fields `cache, cache_dir, image_dims, image_path` | `state/tab.rs::Tab` | Per-tab instead of global |
| `app.rs::App` field `loading, progress` | `App.apply_progress: Option<ApplyProgress>` | Apply-mode only; Background pipelines don't show the bar |
| `app.rs::App` field `mip_fetch_signal` | stays in `App` but typed as `Vec<(TabId, mip, TileRange)>` | Routes to tab on tick |
| `controller.rs::open_file_dialog` | `action/actions/open_file.rs` | `OpenFile` Action |
| `controller.rs::fetch_mip_from_cache` | `action/actions/mip_fetch.rs` | `RequestMipFetch` Action |
| `controller.rs::handle_export_dialog` Export branch | `action/actions/export.rs` | `Export` Action (Apply mode) |
| `file_ops.rs::open_and_run` | folded into `actions/open_file.rs::OpenFile::prepare` | Pipeline construction inline |
| `file_ops.rs::fetch_mip` | folded into `actions/mip_fetch.rs` | |
| `file_ops.rs::export_file` | folded into `actions/export.rs::Export::prepare` | |
| `file_ops.rs` | **deleted** | |
| `tab_bar.rs::State.tabs: Vec<String>` | derived view of `EditorState.tabs` | Tab Msg → `Action::SwitchTab` / `CloseTab` |
| `pixors-executor/src/sink/viewport_cache_sink.rs` ViewportCacheSink | adds `routing_key: u64` field | Callback gets key |
| `pixors-executor/src/sink/tile_sink.rs` TileSink | adds `routing_key: u64` field | Same |
| `pixors-executor/src/sink/cache_writer.rs` | unchanged | already self-contained (cache_dir per instance) |

### App after refactor (sketch)

```rust
pub struct App {
    pub state: EditorState,                     // ← new (tabs, active, pipeline_lock)

    // UI ephemeral (not part of EditorState)
    pub panes: pane_grid::State<PaneKind>,
    pub workspace: workspace_bar::State,
    pub tools: toolbar::State,
    pub layers_ui: layers_panel::State,
    pub filters: filters_panel::State,
    pub status: status_bar::State,
    /// Some(progress) only while an Apply-mode pipeline is running.
    /// Drives the loading bar; UI is also locked when Some.
    pub apply_progress: Option<ApplyProgress>,
    pub errors: Vec<(String, Instant)>,
    pub tile_generation: u64,
    pub mip_fetch_signal: Arc<Mutex<Vec<(TabId, u32, TileRange)>>>,
    pub show_export_dialog: bool,
    pub export_dialog: ExportDialog,
    /// Pipeline → Action coupling. When PipelineEvent::Done arrives,
    /// look up the in-flight action by tab and call its `apply`.
    pub pending_actions: HashMap<TabId, Box<dyn Action>>,
}

pub struct ApplyProgress {
    pub tab: TabId,
    pub progress: f32,
    pub label: String,                          // e.g. "Applying Gaussian Blur..."
}

pub enum Msg {
    Action(Box<dyn Action>),                    // user intents
    PaneResized(...),                           // UI-only
    ShowExportDialog,
    HideExportDialog,
    PipelineEvent(PipelineEvent),               // pipeline finished/progressed
    Tick,                                       // 30Hz
    Frames,
    KeyPressed(...),
    TabBar(tab_bar::Msg),
    // ...
}
```

`update(msg)` flow:

```rust
match msg {
    Msg::Action(action) => {
        // Reject if Apply-mode lock is set and this action would mutate the locked tab.
        if self.state.is_locked() && action_would_mutate_locked_tab(&action, &self.state) {
            self.push_error("Pipeline running, please wait".into());
            return;
        }
        let dispatcher = Dispatcher { event_tx: &pipeline_event_tx() };
        if let Err(e) = dispatcher.dispatch(action, &mut self.state) {
            self.push_error(e);
        }
    }
    Msg::PipelineEvent(PipelineEvent::Done) => {
        // Look up pending action(s), call apply(Done), push HistoryEntry, clear lock.
    }
    Msg::PipelineEvent(PipelineEvent::Error(e)) => {
        // Same but apply(Error) → triggers undo + rollback.
    }
    // ... UI-only mutations
}
```

## Tab bar wiring

After refactor, `tab_bar` becomes a **pure view** function over `EditorState`:

```rust
pub fn view(state: &EditorState) -> Element<TabBarMsg> {
    let active = state.active_id();
    let tabs: Vec<_> = state.tabs().iter().map(|t| {
        // render tab with t.title, highlight if Some(t.id) == active
    }).collect();
    // ...
}

pub enum TabBarMsg {
    Select(TabId),
    Close(TabId),
    Reorder { from: usize, to: usize },
    NewBlank,
}
```

Mapped at app level:
```rust
TabBarMsg::Select(id) => Msg::Action(Box::new(SwitchTab(id))),
TabBarMsg::Close(id)  => Msg::Action(Box::new(CloseTab(id))),
TabBarMsg::NewBlank   => Msg::Action(Box::new(NewBlankTab { width: 1024, height: 1024 })),
```

## Viewport wiring

`ViewportProgram` (the renderer) currently reads from `App.cache: Option<Arc<Mutex<ViewportCache>>>`. After refactor, it reads from the **active tab's cache**:

```rust
let active_cache = state.active_tab().map(|t| t.viewport_cache.clone());
ViewportProgram::new(active_cache, mip_fetch_signal.clone(), state.active_id());
```

When the user switches tabs, the viewport widget gets the new tab's cache. The renderer's `take_new_img()` polling triggers camera reset for the new dimensions.

The `mip_fetch_signal` becomes typed as `Vec<(TabId, u32, TileRange)>` — the renderer pushes its own `TabId` so the dispatcher routes correctly. On tick, the App drains the signal and dispatches `RequestMipFetch` actions per entry.

## Why this design

- **Why a separate `state/` module?** Keeps the editor's persistent data isolated from iced UI ephemera (panes, drag state, dialog open). State could be serialized for session save/restore later.
- **Why `prepare → apply → undo` instead of single `dispatch`?** Splitting lets the dispatcher decide whether to spawn or run inline (StateOnly) and lets `apply` see the pipeline outcome. `undo` as a peer (not a synthesized inverse) keeps complex Apply-mode rollbacks honest: filters can't be inverted by replay, only by snapshot restore. The same trait shape works for trivial `SwitchTab` (no pipeline, no snapshot) and heavy `ApplyFilter` (snapshot before, restore on failure).
- **Why Background vs Apply modes?** Initial file loads, prefetches, MIP fills should not block the UI — the user expects to scroll/zoom while tiles arrive. Filter commits, color conversion, export must be atomic — partial output is worse than waiting. The mode is the action's choice, declared in `prepare`.
- **Why HistoryCache as an entity?** Apply-mode failure is the hard case: the pipeline already wrote partial tiles. The state has to be reconstructable without rerunning. A dedicated cache (snapshots before each Apply) gives `undo` and rollback a single source of truth.
- **Why per-tab caches with routing key?** The executor stays serializable (sinks are `Serialize`/`Deserialize`). Putting `Arc<Mutex<ViewportCache>>` directly in the sink would break that. A `u64` routing key + global router is the minimal cost.
- **Why metadata-first opens?** The user sees a tab + filename + dimensions instantly. Pixels stream in. The split between `Image::open` (meta) and `Image::open_page` (stream) in `pixors-executor/src/common/image/mod.rs` was made for exactly this.
- **Why not `Rc<RefCell>`?** `Tab.viewport_cache` is shared with sink threads. `Arc<Mutex>` is required.
- **Why `Tab` and not `Document`?** A tab is the user-facing concept. "Document" implies persisted-on-disk semantics that we don't yet have (we open files but don't save edits as documents). Calling it `Tab` makes the lifecycle obvious: a tab exists from open to close.

## Implementation phases

### Phase 1 — State skeleton + sink routing (this PR)

- `state/tab.rs`, `state/editor.rs`, `state/history.rs`, `state/mod.rs` — types, `EditorState::new()`, push/close/switch, `pipeline_lock`, `HistoryCache` skeleton.
- `pixors-executor`: add `routing_key: u64` to `ViewportCacheSink` + `TileSink`. Update callback signatures.
- `pixors-desktop`: `sinks/router.rs` — `register_tab_cache`, `unregister_tab_cache`, `get_tab_cache`, `install()`.
- `App` gains `state: EditorState`. Old single-image fields removed; tabs replace them.
- `main.rs` calls `sinks::router::install()` once at startup.

### Phase 2 — Action trait + `OpenFile` (this PR)

- `action/mod.rs`: `Action` trait (`prepare/apply/undo`), `PreparedAction`, `PipelineMode`, `PipelineStatus`, `Dispatcher`.
- `action/actions/open_file.rs`: `OpenFile` Action (Background mode).
- Replaces `controller::open_file_dialog`. Creates a `Tab`, registers its cache, spawns pipeline.
- `tab_bar` re-rendered from `state.tabs()`.
- Viewport reads from `state.active_tab().viewport_cache`.
- Existing placeholder tabs removed; tabs only exist when a real Tab is opened.

### Phase 3 — `SwitchTab` + `CloseTab`

- Tab clicks dispatch actions. ViewportProgram swaps cache reference.
- Closing a tab unregisters its cache + drops `Tab` (and its on-disk cache_dir, optionally).

### Phase 4 — `RequestMipFetch` + `Export`

- Migrate `fetch_mip_from_cache` and export branch into actions. `Export` runs in **Apply mode** — UI locks during export, snapshot taken (cheap because export doesn't mutate state, but the lock semantics still apply).

### Phase 5 — UI lock plumbing

- App reads `state.is_locked()` to:
  - Render the loading bar (already wired).
  - Disable menu items / keyboard shortcuts that dispatch mutating actions on the locked tab.
  - Show "Pipeline running..." toast on rejected attempts.

### Phase 6+ (future, not this PR)

- Layers + edit chain plumbing (LayerSource → pipeline operation translation).
- `ApplyFilter` action with full snapshot/rollback. First real Apply-mode user-facing action.
- Undo/Redo via `History.past`/`History.future`.
- `NewBlankTab` action — pure StateOnly + no pipeline.
- Multipage navigation (PNG APNG / TIFF multi-image: `LayerSource::FilePage { page: n }` for `n > 0`).

## Verification

After phases 1–3:

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
PIXORS_DEV=1 cargo run -p pixors-desktop
```

Manual smoke:
1. Open file A → tab "A.png" appears, tiles render.
2. Open file B → tab "B.png" appears, becomes active, tiles render.
3. Click tab A → viewport switches back to A's tiles immediately (cache preserved).
4. Close tab A → only B remains.
5. Re-open A → fresh load, no stale tiles from old session.
6. While Background pipeline (initial open) runs on tab A, switch to tab B and scroll — UI stays responsive (no lock).

After phase 4–5 (Export = Apply mode):

7. Trigger Export on tab A → loading bar shows, all tab/menu actions on A bounce with toast; tab switch still works (read-only).
8. Force an export error (read-only output dir) → state unchanged, lock cleared, toast shown.

## Out of scope (this PR)

- Layer pipeline composition (compositing multiple layers into the displayed result).
- Real undo/redo with `ApplyFilter` rollback path (skeleton types only).
- `NewBlankTab` actual pixel buffer init (just creates the Tab entry).
- Per-tab edit chain → pipeline translation.
- Session persistence (serializing `EditorState` to disk).
- Disk archive format for `HistoryCache` snapshots.
