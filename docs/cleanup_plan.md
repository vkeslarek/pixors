# Pixors Cleanup Plan — P0–P6

Detailed implementation guide with code examples. Execute in PR order — see sequencing constraints at the bottom.

---

## PR 1 — P0: Correctness Bugs

### B1 — Progress events without tab routing

**Problem:** `PipelineEvent::Progress` has no `TabId` field. `controller.rs:60-63` applies the same progress value to every tab that happens to be loading.

```rust
// CURRENT — pixors-desktop/src/controller.rs:60
PipelineEvent::Progress(n) => {
    for tab in &mut self.state.tabs {
        if tab.view.loading {
            tab.view.progress = n;  // BUG: all loading tabs get same value
        }
    }
}
```

**Fix:**

Step 1 — add `tab` field to `Progress` variant in `pixors-engine/src/runtime/event.rs`:

```rust
// pixors-engine/src/runtime/event.rs
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Progress { tab: Option<TabId>, value: f32 },  // was: Progress(f32)
    Done { tab: Option<TabId> },
    Error { tab: Option<TabId>, message: String },
    Cancelled { tab: Option<TabId> },
}
```

Step 2 — thread `tab` through `ChainRunner` in `pixors-engine/src/runtime/chain.rs`:

```rust
// pixors-engine/src/runtime/chain.rs
// ChainRunner carries the routing tab (set from the graph's routed_tab)
pub struct ChainRunner {
    // ... existing fields
    pub tab: Option<TabId>,
}

// in progress reporting:
let _ = self.progress_tx.try_send(PipelineEvent::Progress {
    tab: self.tab,
    value: n,
});
```

Step 3 — fix the controller:

```rust
// pixors-desktop/src/controller.rs
PipelineEvent::Progress { tab, value } => {
    match tab {
        Some(id) => {
            if let Some(t) = self.state.tabs.iter_mut().find(|t| t.id == id) {
                t.view.progress = value;
            }
        }
        None => {
            // fallback: update all loading tabs (should not happen after this fix)
            for t in &mut self.state.tabs {
                if t.view.loading { t.view.progress = value; }
            }
        }
    }
}
```

---

### B2 — TileReadFn closure leak per blur preview tick

**Problem:** `blur_preview.rs:52-83` calls `install_viewport_cache_reader(tab_id, reader_fn)` every time a preview is dispatched. Only `CloseTab` uninstalls. Dragging the slider 50× = 50 live closures, each holding `Arc<Mutex<TileCache>>`.

```rust
// CURRENT — pixors-state/src/action/actions/blur_preview.rs:52
// Called every preview tick
let cache = tab.viewport_cache.clone();
install_viewport_cache_reader(tab.id, move |pos, gen| {
    cache.lock().unwrap().get_tile(pos, gen)
});
// BUG: previous reader for this tab is overwritten but the Arc<Mutex> it held
// is NOT dropped until the global registry is overwritten. If install replaces
// by key, it's fine — but verify the registry semantics:
```

Check `viewport_cache_source.rs` (to be renamed `tile_cache_source.rs`):

```rust
// pixors-state/src/viewport_cache_source.rs
static READERS: RwLock<HashMap<u64, TileReadFn>> = ...;

pub fn install_viewport_cache_reader(tab: TabId, f: TileReadFn) {
    READERS.write().unwrap().insert(tab.0, f); // replaces old one — Arc dropped here
}
```

If it's a `HashMap::insert`, the old closure IS dropped. The real leak is that if `BlurPreview::apply()` is never called (pipeline cancelled), there's no cleanup path. **Fix:**

```rust
// pixors-state/src/action/actions/blur_preview.rs
impl Action for BlurPreview {
    fn prepare(&mut self, state: &mut EditorState) -> Result<PreparedAction, String> {
        let tab = state.active_tab_mut()?;
        // Install reader ONCE — idempotent if already installed for this tab
        if !is_viewport_cache_reader_installed(tab.id) {
            let cache = tab.tile_cache.clone();
            install_viewport_cache_reader(tab.id, move |pos, gen| {
                cache.lock().unwrap().get_tile(pos, gen)
            });
        }
        // build graph...
    }

    fn apply(&mut self, state: &mut EditorState, status: ActionStatus) {
        if matches!(status, ActionStatus::Cancelled | ActionStatus::Error(_)) {
            // Clear overlay tiles that were being written
            if let Some(tab) = state.tab_mut(self.tab_id) {
                tab.tile_cache.lock().unwrap().clear_overlay(self.generation);
            }
        }
    }
}
```

Add `is_viewport_cache_reader_installed(tab: TabId) -> bool` helper in `tile_cache_source.rs`:

```rust
pub fn is_viewport_cache_reader_installed(tab: TabId) -> bool {
    READERS.read().unwrap().contains_key(&tab.0)
}
```

---

### B3 — Broadcast Lagged drops Done events, tab stays locked

**Problem:** `app.rs:142`:

```rust
// CURRENT — pixors-desktop/src/app.rs
Err(RecvError::Lagged(_)) => continue,  // BUG: may have dropped Done
```

If a `Done` event for tab X is among the lagged messages, `Dispatcher::active_apply_actions` keeps tab X locked until restart.

**Fix (two-part):**

Part A — increase channel capacity in `pixors-engine/src/runtime/pipeline.rs`:

```rust
// pixors-engine/src/runtime/pipeline.rs
// Current: broadcast::channel(16)
let (tx, _) = broadcast::channel(256);  // generous buffer
```

Part B — on `Lagged`, resync from Dispatcher state:

```rust
// pixors-desktop/src/controller.rs or app.rs
Err(RecvError::Lagged(skipped)) => {
    tracing::warn!("pipeline event channel lagged, skipped={skipped}; resyncing tab locks");
    self.dispatcher.resync_locks(&mut self.state);
    continue;
}
```

Add `Dispatcher::resync_locks` in `pixors-state/src/action/dispatcher.rs`:

```rust
// pixors-state/src/action/dispatcher.rs
/// Called when the event channel lags. Clears locks for tabs whose pipeline
/// is no longer running (the thread has exited).
pub fn resync_locks(&mut self, state: &mut EditorState) {
    self.locked_tabs.retain(|tab_id, handle| {
        let still_running = handle.is_running(); // check Arc<AtomicBool> or JoinHandle
        if !still_running {
            if let Some(tab) = state.tab_mut(*tab_id) {
                tab.view.loading = false;
                tab.view.progress = 1.0;
            }
        }
        still_running
    });
}
```

---

### B4 — OpenFile::prepare mutates state before pipeline runs

**Problem:** `open_file.rs:44-187` already calls `state.push_tab(tab)` and `register_tab_cache(tab_id, writer_fn)` inside `prepare()`. If the pipeline fails, the tab stays visible.

**Fix:** Move all `EditorState` mutations to `apply()`. `prepare()` returns the `TabId` in `PreparedAction` metadata.

```rust
// pixors-state/src/action/actions/open_file.rs

pub struct OpenFile {
    path: PathBuf,
    // remove: pending_tab_id: Arc<Mutex<Option<TabId>>>
    allocated_tab_id: TabId,  // allocated in prepare, applied in apply
}

impl Action for OpenFile {
    fn prepare(&mut self, state: &mut EditorState) -> Result<PreparedAction, String> {
        // Validate the path (fast check, no I/O)
        if !self.path.exists() {
            return Err(format!("file not found: {}", self.path.display()));
        }
        // Allocate the TabId now so it can be embedded in the graph's routing key
        let tab_id = state.next_tab_id();
        self.allocated_tab_id = tab_id;

        // Build graph — source stage will open the file lazily in Producer::start
        let image_source = Arc::new(ImageStreamSource::from_path(self.path.clone()));
        let mut builder = PathBuilder::new();
        builder.source(image_source);
        builder.scanline_decode(state.working_meta());
        // disk cache writer
        let cache_dir = state.cache_dir_for(tab_id);
        builder.sink(Arc::new(CacheWriter::new(cache_dir.clone())));
        // viewport tile cache writer
        builder.tile_cache_sink(tab_id, 0);  // gen=0 = base layer

        Ok(PreparedAction::Pipeline {
            mode: PipelineMode::Background,
            graph: builder.build(),
            snapshot: None,
            routed_tab: Some(tab_id),
        })
    }

    fn apply(&mut self, state: &mut EditorState, status: ActionStatus) {
        match status {
            ActionStatus::Done => {
                // NOW we push the tab (pipeline succeeded)
                let desc = /* retrieve from side channel or store on self */ ...;
                let tab = Tab::new(self.allocated_tab_id, self.path.clone(), desc, ...);
                state.push_tab(tab);
                state.set_active(self.allocated_tab_id);
            }
            ActionStatus::Error(msg) => {
                tracing::error!("OpenFile failed: {msg}");
                // no tab was pushed, nothing to clean up
            }
            _ => {}
        }
    }
}
```

The tricky part: `ImageDescriptor` (width, height, format) is discovered during the pipeline's `Producer::start`. Pass it back via an `Arc<Mutex<Option<ImageDescriptor>>>` stored on `OpenFile` — the producer writes it, `apply()` reads it.

```rust
pub struct OpenFile {
    path: PathBuf,
    allocated_tab_id: TabId,
    discovered_desc: Arc<Mutex<Option<ImageDescriptor>>>,
}

// In the ImageStreamSource producer:
fn start(...) -> Result<...> {
    let image = open_image(&self.path)?;
    *self.desc_sink.lock().unwrap() = Some(image.descriptor());
    // emit ScanLines...
}

// In apply():
let desc = self.discovered_desc.lock().unwrap().take()
    .expect("producer must have set desc before Done fires");
```

---

### B5 — BlurPreview/BlurCancel actions never dispatched

**Problem:** `controller.rs:332-372` builds the blur preview pipeline inline, bypassing the `BlurPreview` action:

```rust
// CURRENT — pixors-desktop/src/controller.rs
fn dispatch_blur_preview(&mut self, radius: f32) {
    // ... builds ExecGraph inline here, not using BlurPreview action
    let path = Path::new(vec![
        Arc::new(ViewportCacheSource::new(...)),
        Arc::new(ColorConvert::new(...)),
        // ...
    ]);
    self.active_post_process = Some(path);
    // dispatches RequestMipFetch with the inline path
}
```

**Fix:** Delete the inline builder, dispatch the action:

```rust
// pixors-desktop/src/controller.rs
fn dispatch_blur_preview(&mut self, radius: f32) {
    let action = Arc::new(Mutex::new(BlurPreview { radius }));
    if let Err(e) = self.dispatcher.dispatch(action, &mut self.state) {
        self.push_error(e);
    }
}

fn dispatch_blur_cancel(&mut self) {
    let action = Arc::new(Mutex::new(BlurCancel));
    self.dispatcher.dispatch(action, &mut self.state).ok();
}
```

The `BlurPreview` and `BlurCancel` actions already exist in `pixors-state/src/action/actions/`. They just need to be wired.

---

### B6 — scheduler.rs panic on buffer map failure

```rust
// CURRENT — pixors-engine/src/gpu/scheduler.rs:307
let data = rx.recv().unwrap().unwrap(); // double unwrap

// FIX:
let data = rx.recv()
    .map_err(|_| Error::internal("GPU buffer map channel closed unexpectedly"))?
    .map_err(|e| Error::internal(format!("GPU buffer map failed: {e:?}")))?;
```

---

## PR 2 — P1: Headless Unblocker (Tab: Send + Sync)

### H1 — Remove Rc<RefCell<ViewportState>> from Tab

`Tab: !Send` because `viewport_state: Rc<RefCell<ViewportState>>`.

```rust
// CURRENT — pixors-state/src/state/tab.rs
pub struct Tab {
    // ...
    pub viewport_state: Rc<RefCell<ViewportState>>,  // !Send !Sync — blocks MCP
}
```

`ViewportState` holds: `zoom: f32`, `pan: Vec2`, mouse drag state, `last_bounds: Option<Rectangle>`, `last_visible_range: Option<TileRange>`. Mouse drag and UI bounds are pure desktop concerns.

**Fix in pixors-state:** Strip `viewport_state` from `Tab`. Move `zoom`/`pan` to plain fields:

```rust
// pixors-state/src/state/tab.rs
pub struct Tab {
    // ... remove viewport_state entirely
    pub zoom: f32,           // read by MCP to know what the user sees
    pub pan: Vec2,           // same
    // everything else in ViewportState belongs in desktop
}
```

**Fix in pixors-desktop:** Desktop owns its own render state per tab:

```rust
// pixors-desktop/src/viewport/render_state.rs  (new file)
pub struct ViewportRenderState {
    pub drag_origin: Option<Point>,
    pub last_bounds: Option<Rectangle>,
    pub last_visible_range: Option<TileRange>,
}

impl ViewportRenderState {
    pub fn new() -> Self { Self { drag_origin: None, last_bounds: None, last_visible_range: None } }
}
```

```rust
// pixors-desktop/src/app.rs
pub struct App {
    // ...
    pub viewport_render: HashMap<TabId, ViewportRenderState>,
}

// When a tab is opened:
self.viewport_render.insert(tab.id, ViewportRenderState::new());

// When a tab is closed:
self.viewport_render.remove(&tab_id);
```

`components/viewport.rs` reads `app.state.active_tab()` for zoom/pan and `app.viewport_render[tab_id]` for drag/bounds.

**Acceptance test:**

```rust
// pixors-state/src/state/tab.rs
#[cfg(test)]
mod tests {
    use super::*;
    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn tab_is_send_sync() {
        assert_send_sync::<Tab>();
    }

    #[test]
    fn editor_state_is_send_sync() {
        assert_send_sync::<EditorState>();
    }
}
```

---

### H2 — PathBuilder: strip Rc<RefCell>

```rust
// CURRENT — pixors-state/src/path_builder.rs
pub struct PathBuilder {
    inner: Rc<RefCell<Inner>>,  // gratuitous — lives entirely on the stack
}

// FIX:
pub struct PathBuilder {
    stages: Vec<Arc<dyn Stage>>,
    // or: Inner held directly
}

impl PathBuilder {
    pub fn new() -> Self { Self { stages: vec![] } }

    pub fn add(&mut self, stage: Arc<dyn Stage>) -> &mut Self {
        self.stages.push(stage);
        self
    }

    pub fn build(self) -> ExecGraph<Arc<dyn Stage>> {
        // construct graph from self.stages
    }
}
```

---

## PR 3 — P2: Naming Consistency

All mechanical. Use `cargo fix --workspace` after each rename to update imports, or do it manually.

### Rename map

| Old | New | Notes |
|---|---|---|
| `ViewportCache` | `TileCache` | type + file `viewport_cache.rs` → `tile_cache.rs` |
| `ViewportCacheSource` | `TileCacheSource` | file `viewport_cache_source.rs` → `tile_cache_source.rs` |
| `ViewportCacheSink` | `TileCacheSink` | file `viewport_cache_sink.rs` → `tile_cache_sink.rs` |
| `install_viewport_cache_reader` | `install_tile_cache_reader` | function |
| `register_tab_cache` | `register_tile_cache` | function |
| `uninstall_viewport_cache_reader` | `uninstall_tile_cache_reader` | function |
| `Tab.tile_generation` | `Tab.redraw_seq` | display invalidation counter |
| `CachedTile.generation` | `CachedTile.layer` | 0 = base, >0 = preview overlay |
| `Tab.mip_fetch_signal` | `Tab.mip_fetch_queue` | it's a queue, not a signal |
| `Tab.tile_generation.wrapping_add(1)` | `Tab.redraw_seq += 1` | no wrapping needed with u64 |

### Drop dead TabView fields

```rust
// CURRENT — pixors-state/src/state/tab.rs
pub struct TabView {
    pub loading: bool,
    pub progress: f32,
    pub active_mip: u32,
    pub zoom: f32,     // never read — zoom lives on Camera
    pub pan: Vec2,     // never read
    pub preview_gen: u64, // never read (removed in PR 1/2)
}

// AFTER:
pub struct TabView {
    pub loading: bool,
    pub progress: f32,
    pub active_mip: u32,
}
```

### Verification

```bash
rg -w 'ViewportCache|ViewportCacheSource|ViewportCacheSink|install_viewport_cache_reader|register_tab_cache|tile_generation|mip_fetch_signal|preview_gen' \
   --type rust
# must return zero hits
```

---

## PR 4 — P3: Dead Code Purge

### Delete png_encoder.rs

```bash
# pixors-image/src/sink/mod.rs — remove line:
pub mod png_encoder;

# Delete the file:
rm pixors-image/src/sink/png_encoder.rs
```

Confirm only `png_encoder_v2` is imported:

```bash
rg 'PngEncoder[^V]' --type rust  # should return zero (only PngEncoderV2 used)
```

### Delete dead engine items

```rust
// pixors-engine/src/stage/node.rs — remove:
pub enum StageRole { Source, Operation, Sink }  // never matched outside module

// pixors-engine/src/data/tile.rs — remove:
impl TileCoord {
    pub fn pixel_count(&self) -> u32 { ... }  // no callers
    pub fn bounds(&self) -> ... { ... }         // no callers
}

// pixors-engine/src/data/neighborhood.rs — remove:
pub struct NeighborhoodCoord { ... }  // unused
impl Neighborhood {
    pub fn tile_at(&self, ...) { ... }  // no callers
}

// pixors-engine/src/data_transform/to_neighborhood.rs:72-73 — remove:
gpu_tile_w: u32,  // written at 355, never read
gpu_tile_h: u32,  // written at 356, never read
```

### Drop AlphaMode from pixors-image

```rust
// pixors-image/src/common/image/mod.rs — remove:
pub enum AlphaMode { Straight, Premultiplied, None }
// Replace all usages with pixors_engine::AlphaPolicy
```

### Strip dead state types

```rust
// pixors-state/src/state/tab.rs — remove:
pub struct EditChain { pub ops: Vec<()> }  // placeholder, never used

// pixors-state/src/state/editor.rs — remove:
pub pipeline_lock: Option<TabId>  // locking is in Dispatcher; this field is always None

// pixors-state/src/action/mod.rs — remove snapshot from PreparedAction:
PreparedAction::Pipeline {
    mode: PipelineMode,
    graph: ExecGraph<Arc<dyn Stage>>,
    // snapshot: Option<SnapshotId>,  // remove — always None, history not wired yet
    routed_tab: Option<TabId>,
}
```

### Document dormant history module

```rust
// pixors-state/src/action/mod.rs — near dispatch():
// NOTE(history): record_in_history() is intentionally unimplemented.
// history.rs (EditorHistory, Snapshot, LayerSnapshot) is dormant until
// undo/redo is prioritized. Do not remove either — they mark intent and
// will be wired when the Action::undo path is needed.
// Tracking issue: https://github.com/owner/pixors/issues/XXX
if action.record_in_history() {
    // TODO: push HistoryEntry — see comment above
}
```

### use_compression dead field

```rust
// pixors-image/src/sink/cache_writer.rs — remove:
pub struct CacheWriter {
    // use_compression: bool,  // always false, never read
    ...
}

// pixors-ops/src/source/cache_reader.rs — remove matching field
```

### Cargo.toml hygiene

```toml
# pixors-engine/Cargo.toml — remove:
kamadak-exif = "0.6.1"   # imported only by pixors-image
tracing-subscriber = ...  # init_tracing() is being deleted

# pixors-image/Cargo.toml — remove:
wgpu = ...                # no wgpu:: in src/

# pixors-desktop/Cargo.toml — verify then remove:
wide = ...    # grep for 'wide::' in pixors-desktop/src/ — if zero hits, drop
half = ...    # same
rayon = ...   # same
```

### Move shared deps to workspace

```toml
# Cargo.toml (workspace root) — add [workspace.dependencies]:
[workspace.dependencies]
wide = "0.7"
half = "2"
bytemuck = { version = "1", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
tracing = "0.1"
wgpu = { version = "22", features = ["spirv"] }
rayon = "1"
lz4_flex = "0.11"
parking_lot = "0.12"

# Each member Cargo.toml that uses these:
[dependencies]
wide = { workspace = true }   # instead of wide = "0.7"
# etc.
```

---

## PR 5 — P4: Action Layer Hardening

### A2 — SwitchTab / BlurCancel don't need Action trait

Add a direct dispatch path for simple state mutations that don't need history or pipeline:

```rust
// pixors-state/src/action/dispatcher.rs
impl Dispatcher {
    /// For instantaneous, non-undoable state mutations.
    pub fn mutate<F>(&mut self, state: &mut EditorState, f: F)
    where
        F: FnOnce(&mut EditorState),
    {
        f(state);
    }
}
```

```rust
// pixors-desktop/src/controller.rs — SwitchTab:
// BEFORE:
self.dispatcher.dispatch(Arc::new(Mutex::new(SwitchTab { id })), &mut self.state);

// AFTER:
self.dispatcher.mutate(&mut self.state, |s| s.set_active(id));
```

Keep `SwitchTab` in `pixors-state` as a module for a bit longer in case MCP needs it (it's a tiny action), but remove the `Action` impl and just expose `EditorState::set_active(id)` publicly.

### A5 — PathBuilder dedup key

```rust
// pixors-state/src/path_builder.rs:94
// BEFORE:
let key = format!("{:?}", stage);

// AFTER:
let key = Arc::as_ptr(&stage) as usize;
```

### A6 — Async file dialog

```rust
// pixors-desktop/src/controller.rs
// BEFORE (blocks UI thread):
fn open_file_dialog(&self) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Images", &["png", "tif", "tiff"])
        .pick_file()
}

// AFTER (async):
fn open_file_dialog(&mut self) -> iced::Command<Msg> {
    iced::Command::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("Images", &["png", "tif", "tiff"])
                .pick_file()
                .await
                .map(|h| h.path().to_owned())
        },
        |result| Msg::FileDialogResult(result),
    )
}
```

Add `Msg::FileDialogResult(Option<PathBuf>)` to `app.rs` and handle it in the controller.

---

## PR 6 — P5: Dedup Helpers + Engine Cleanup

### D1 — Single TILE_SIZE constant

```rust
// pixors-state/src/lib.rs (or a new pixors-state/src/constants.rs)
pub const TILE_SIZE: u32 = 256;
```

Remove from: `open_file.rs:22`, `blur_preview.rs:23`, `export.rs:15`, `mip_fetch.rs:14`, `pixors-desktop/src/viewport/program.rs:16`. All import from `pixors_state::TILE_SIZE`.

### D2 — PathBuilder helpers

```rust
// pixors-state/src/path_builder.rs

impl PathBuilder {
    /// Append: ImageStreamSource → ScanLineToTile → ColorConvert(working).
    /// Requires the source to already be set.
    pub fn scanline_decode(&mut self, image: Arc<ImageStreamSource>, meta: PixelMeta) -> &mut Self {
        self.add(image)
            .add(Arc::new(ScanLineToTile::new(TILE_SIZE)))
            .add(Arc::new(ColorConvert::to(meta)))
    }

    /// Append a TileCacheSink targeting an in-memory tab cache at a given generation.
    pub fn tile_cache_sink(&mut self, tab: TabId, generation: u64) -> &mut Self {
        self.add(Arc::new(TileCacheSink::new(tab.0, generation)))
    }

    /// Append ColorConvert to display meta + TileCacheSink (common tail for preview pipelines).
    pub fn to_display_cache(&mut self, tab: TabId, generation: u64, display: PixelMeta) -> &mut Self {
        self.add(Arc::new(ColorConvert::to(display)))
            .tile_cache_sink(tab, generation)
    }
}
```

Usage in `open_file.rs`:

```rust
// BEFORE (scattered across all actions):
builder.source(img_src.clone());
builder.add(Arc::new(ScanLineToTile::new(256)));
builder.add(Arc::new(ColorConvert::new(working_format, working_cs, AlphaPolicy::Straight)));
// ...
builder.sink(Arc::new(TileCacheSink::new(tab.id.0, 0)));

// AFTER:
builder.scanline_decode(img_src, state.working_meta());
// ...
builder.tile_cache_sink(tab_id, 0);
```

### D3 — consolidate_tiles helper

Both `upload.rs` and `to_neighborhood.rs` do:
1. Iterate `&[Tile]`
2. Allocate a contiguous GPU buffer big enough for all tile data
3. `copy_slice` each tile's bytes in
4. Build `Vec<TileGpuInfo>` with offsets

Extract to `pixors-engine/src/data_transform/consolidate.rs`:

```rust
// pixors-engine/src/data_transform/consolidate.rs

pub struct ConsolidatedGpuTiles {
    pub buffer: Arc<GpuBuffer>,
    pub tile_infos: Vec<TileGpuInfo>,
}

pub fn consolidate_tiles(
    scheduler: &Scheduler,
    tiles: &[Tile],
) -> Result<ConsolidatedGpuTiles, Error> {
    let total_bytes: usize = tiles.iter().map(|t| t.byte_len()).sum();
    let buffer = scheduler.alloc_zeroed_buffer(total_bytes)?;

    let mut tile_infos = Vec::with_capacity(tiles.len());
    let mut offset = 0usize;

    for tile in tiles {
        let bytes = tile.data.as_cpu_bytes()
            .ok_or_else(|| Error::internal("consolidate_tiles: expected CPU buffer"))?;
        scheduler.copy_slice_to_buffer(&buffer, offset, bytes)?;
        tile_infos.push(TileGpuInfo {
            px: tile.coord.px,
            py: tile.coord.py,
            width: tile.coord.width,
            height: tile.coord.height,
            data_offset: offset as u64,
            tile_size_bytes: bytes.len() as u64,
        });
        offset += bytes.len();
    }

    Ok(ConsolidatedGpuTiles { buffer: Arc::new(buffer), tile_infos })
}
```

Replace the duplicated bodies in `upload.rs:88-125` and `to_neighborhood.rs:206-308` with a call to this helper.

### D4 — Shared tile-grid assembler in pixors-image

```rust
// pixors-image/src/sink/mod.rs

/// Assemble a flat buffer of full-image pixels from a collection of tiles.
/// Tiles may be in any order; missing tiles are left as zeroed bytes.
pub(crate) fn assemble_tile_grid(
    tiles: &[Tile],
    image_width: u32,
    image_height: u32,
    bytes_per_pixel: usize,
) -> Vec<u8> {
    let total = (image_width as usize) * (image_height as usize) * bytes_per_pixel;
    let mut out = vec![0u8; total];
    let row_stride = (image_width as usize) * bytes_per_pixel;

    for tile in tiles {
        let tile_bytes = tile.data.as_cpu_bytes().expect("assemble_tile_grid: need CPU buffer");
        let tile_stride = (tile.coord.width as usize) * bytes_per_pixel;
        for row in 0..tile.coord.height as usize {
            let src_start = row * tile_stride;
            let dst_row = tile.coord.py as usize + row;
            let dst_col = tile.coord.px as usize;
            let dst_start = dst_row * row_stride + dst_col * bytes_per_pixel;
            let len = tile_stride.min(row_stride - dst_col * bytes_per_pixel);
            out[dst_start..dst_start + len]
                .copy_from_slice(&tile_bytes[src_start..src_start + len]);
        }
    }
    out
}
```

Replace identical bodies in `png_encoder_v2.rs:110-156` and `tiff_encoder.rs:110-156`.

### D6 — Color LUT instead of 16-arm match

```rust
// pixors-color/src/operation/color.rs

type KernelFn = fn(/* params */) -> Result<(), Error>;

static KERNEL_TABLE: [[KernelFn; 4]; 4] = [
    // [src_prec][dst_prec]
    [kernel_u8_u8,  kernel_u8_u16,  kernel_u8_f16,  kernel_u8_f32 ],
    [kernel_u16_u8, kernel_u16_u16, kernel_u16_f16, kernel_u16_f32],
    [kernel_f16_u8, kernel_f16_u16, kernel_f16_f16, kernel_f16_f32],
    [kernel_f32_u8, kernel_f32_u16, kernel_f32_f16, kernel_f32_f32],
];

fn dispatch_kernel(src_prec: Precision, dst_prec: Precision, /* ... */) -> Result<(), Error> {
    KERNEL_TABLE[src_prec as usize][dst_prec as usize](/* params */)
}
```

Replace both 16-arm `match` blocks in `color.rs`.

---

## PR 7 — P6: Robustness

### R1 — assign_devices: add iteration cap

```rust
// pixors-engine/src/runtime/pipeline.rs

fn assign_devices(graph: &mut StableDiGraph<..>) {
    let max_iterations = graph.node_count() * 3;
    let mut iterations = 0usize;

    loop {
        iterations += 1;
        if iterations > max_iterations {
            tracing::warn!(
                "assign_devices: fixed-point did not converge after {iterations} passes; \
                 remaining Either nodes will default to GPU"
            );
            // assign GPU to all remaining Either nodes
            for node in graph.node_weights_mut() {
                if node.hints().device == Device::Either {
                    node.assigned_device = Device::Gpu;
                }
            }
            break;
        }

        let changed = /* ... one pass ... */;
        if !changed { break; }
    }
}
```

Add unit tests:

```rust
#[cfg(test)]
mod assign_devices_tests {
    #[test]
    fn cpu_gpu_either_assigns_correctly() {
        // Source(GPU) -> Op(Either) -> Sink(CPU)
        // Expected: Op -> CPU (minimize transfers)
        let mut graph = build_test_graph(&[Device::Gpu, Device::Either, Device::Cpu]);
        assign_devices(&mut graph);
        assert_eq!(graph[op_node].assigned_device, Device::Cpu);
    }

    #[test]
    fn all_either_defaults_to_gpu() {
        let mut graph = build_test_graph(&[Device::Either, Device::Either]);
        assign_devices(&mut graph);
        for node in graph.node_weights() {
            assert_eq!(node.assigned_device, Device::Gpu);
        }
    }

    #[test]
    fn converges_within_3n_iterations() {
        // Long chain of Either nodes
        let n = 100;
        let devices = vec![Device::Either; n];
        let mut graph = build_test_graph(&devices);
        assign_devices(&mut graph); // must not warn, must terminate
    }
}
```

### R2 — merge_inputs: join spawned threads

```rust
// pixors-engine/src/runtime/pipeline.rs
// BEFORE: spawns and forgets
std::thread::spawn(move || { ... });

// AFTER: use scoped threads so they're joined before the function returns
std::thread::scope(|s| {
    for input_chain in input_chains {
        s.spawn(|| { run_chain(input_chain) });
    }
    // scope exit joins all
});
```

### R3 — chain: recursive finish → iterative

```rust
// pixors-engine/src/runtime/chain.rs
// BEFORE (recursive):
fn run_finish_streaming(stage: &dyn Processor, ctx: &mut ProcessorContext) -> Result<(), Error> {
    stage.finish(ctx)?;
    for next in ctx.next_stages() {
        run_finish_streaming(next, ctx)?;  // could stack-overflow on long chains
    }
    Ok(())
}

// AFTER (iterative):
fn run_finish_streaming(stages: &[Arc<dyn Processor>], ctx: &mut ProcessorContext) -> Result<(), Error> {
    let mut queue: VecDeque<Arc<dyn Processor>> = stages.iter().cloned().collect();
    while let Some(stage) = queue.pop_front() {
        let emitted = stage.finish(ctx)?;
        queue.extend(emitted);
    }
    Ok(())
}
```

### R4 — chain: replace polling with blocking recv

```rust
// pixors-engine/src/runtime/chain.rs
// BEFORE:
loop {
    match rx.recv_timeout(Duration::from_millis(100)) {
        Ok(Some(item)) => process(item),
        Ok(None) => break,
        Err(RecvTimeoutError::Timeout) => continue,  // wastes wakeups
        Err(e) => return Err(e.into()),
    }
}

// AFTER:
loop {
    match rx.recv() {  // blocks until item or disconnect
        Ok(Some(item)) => process(item),
        Ok(None) => break,
        Err(e) => return Err(e.into()),
    }
}
// Cancellation: producer checks AtomicBool and sends None or drops sender
```

### R6 — mip_downsample: remove magic abort cap

```rust
// pixors-ops/src/operation/mip_downsample.rs
// BEFORE:
for _ in 0..50 {  // abort cap masks bugs
    if pending.is_empty() { break; }
    flush_one(&mut pending, &mut output);
}

// AFTER:
flush_all(&mut pending, &mut output);
debug_assert!(pending.is_empty(), "flush_remaining must drain all pending blocks");
```

The invariant: every 2×2 block that was started must be completable at `finish()`. If not, it's a pipeline construction bug (tile count not divisible by 4). Assert instead of silently aborting.

### R7 — global RwLock: use parking_lot + avoid poison panic

```rust
// pixors-state/src/tile_cache_source.rs (renamed)
use parking_lot::RwLock;  // infallible — no poisoning

static READERS: RwLock<HashMap<u64, TileReadFn>> = RwLock::new(HashMap::new());

pub fn install_tile_cache_reader(tab: TabId, f: TileReadFn) {
    READERS.write().insert(tab.0, f);  // no .unwrap() needed
}

pub fn uninstall_tile_cache_reader(tab: TabId) {
    READERS.write().remove(&tab.0);
}
```

### R10 — viewport: remove 1px jitter

```rust
// pixors-desktop/src/components/viewport.rs:37
// BEFORE (causes visible jitter every other frame):
let pad = if tile_generation.is_multiple_of(2) { 0.0 } else { 1.0 };

// AFTER: use fixed padding based on subpixel alignment (or just remove if unneeded):
let pad = 0.0f32;  // or derive from actual tile bounds
```

---

## PR 8 — P7 Docs Sweep + P8 Test Floor

### Docs

Mark stale files with a banner rather than deleting, so history is preserved:

```markdown
<!-- docs/ARCHITECTURE.md — add at top: -->
> **STALE** — this document describes `pixors-executor` which was split into
> `pixors-engine`, `pixors-color`, `pixors-image`, `pixors-ops`, `pixors-state`
> in May 2026. See `CLAUDE.md` and `AGENTS.md` for current architecture.
```

Files to update:
- `docs/ARCHITECTURE.md` — add stale banner
- `docs/ui-functioning.md` — rewrite to describe current `pixors-state`/`pixors-desktop` split
- `docs/ENGINE_MIGRATION.md` — add stale banner (migration is complete)
- `README.md` — remove `pixors-ui` references; describe `pixors-desktop` and `pixors-mcp`
- `CLAUDE.md` — finish removing `pixors-ui` and `make build-front`

Verification:

```bash
rg -i 'pixors-executor|pixors-ui|build-front' README.md CLAUDE.md AGENTS.md docs/
# must return zero hits
```

### Tests: pixors-state dispatcher

```rust
// pixors-state/tests/dispatcher.rs
use pixors_state::{EditorState, Dispatcher, action::{Action, PreparedAction, PipelineMode}};
use std::sync::{Arc, Mutex};

struct FakeAction { called: Arc<Mutex<bool>> }

impl Action for FakeAction {
    fn prepare(&mut self, state: &mut EditorState) -> Result<PreparedAction, String> {
        Ok(PreparedAction::StateOnly)
    }
    fn apply(&mut self, _state: &mut EditorState, _status: pixors_state::action::ActionStatus) {
        *self.called.lock().unwrap() = true;
    }
    fn undo(&mut self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool { false }
}

#[test]
fn state_only_action_calls_apply() {
    let mut state = EditorState::default();
    let mut dispatcher = Dispatcher::default();
    let called = Arc::new(Mutex::new(false));
    let action = Arc::new(Mutex::new(FakeAction { called: called.clone() }));

    dispatcher.dispatch(action, &mut state).expect("dispatch failed");
    assert!(*called.lock().unwrap(), "apply must be called for StateOnly");
}

#[test]
fn concurrent_pipeline_on_same_tab_is_rejected() {
    // ... build a slow pipeline action, dispatch it, dispatch again before Done
    // second dispatch must return Err (tab locked)
}
```

### Tests: pixors-ops compose

```rust
// pixors-ops/tests/compose.rs
#[test]
fn alpha_over_with_full_alpha_passthrough() {
    // src: fully opaque red [255, 0, 0, 255]
    // dst: fully opaque blue [0, 0, 255, 255]
    // over(src, dst) must be red [255, 0, 0, 255]
    let result = alpha_over([255, 0, 0, 255], [0, 0, 255, 255]);
    assert_eq!(result, [255, 0, 0, 255]);
}

#[test]
fn alpha_over_with_zero_alpha_passthrough() {
    // src: fully transparent [255, 0, 0, 0]
    // dst: fully opaque blue
    // over(src, dst) must be blue
    let result = alpha_over([255, 0, 0, 0], [0, 0, 255, 255]);
    assert_eq!(result, [0, 0, 255, 255]);
}
```

### Tests: pixors-color round-trip

```rust
// pixors-color/tests/round_trip.rs
#[test]
fn srgb_to_acescg_roundtrip_within_tolerance() {
    let srgb_input = Rgba::<f32>::new(0.5, 0.3, 0.8, 1.0);
    let working = convert_pixel(srgb_input, ColorSpace::SRGB, ColorSpace::ACES_CG);
    let back = convert_pixel(working, ColorSpace::ACES_CG, ColorSpace::SRGB);
    let delta_e = delta_e_2000(srgb_input, back);
    assert!(delta_e < 0.001, "ΔE={delta_e} exceeds tolerance");
}
```

### Tests: pixors-image codec round-trip

```rust
// pixors-image/tests/codec_roundtrip.rs
#[test]
fn png_encode_decode_roundtrip() {
    let original = make_test_image(1024, 1024, PixelFormat::Rgba8);
    let mut buf = Vec::new();
    PngEncoderV2::encode(&original, &mut buf).unwrap();
    let decoded = PngDecoder::decode(&buf[..]).unwrap();
    assert_eq!(original.descriptor(), decoded.descriptor());
    assert_eq!(original.pixels(), decoded.pixels());
}
```

---

## Sequencing Constraints

```
PR 1 (P0 bugs) ─────────────────────────────────────────────────────┐
PR 2 (P1 headless) ──────────────────────────────────────────────── │
PR 3 (P2 renames)    must come after PR 2                           │
PR 4 (P3 dead code)  must come after PR 3                           │
PR 5 (P4 actions)    must come after PR 3 (uses renamed types)      │
PR 6 (P5 dedup)      must come after PR 4 + PR 5                    │
PR 7 (P6 robustness) must come after PR 6 (cleanup duplicates first)│
PR 8 (docs + tests)  must come last (docs describe final state)     │
```

PRs 1 and 2 can be developed in parallel (touch different files) and merged in order.

---

## Deferred: MCP Server Design

`pixors-mcp/src/index.ts` calls `http://127.0.0.1:8080` which does not exist. Blocked until a design decision is made between:
1. `pixors-server` crate (axum + WS) — real-time bidirectional; good for shared session between desktop + MCP.
2. Stdio + JSON-RPC binary — simpler, fully self-contained, easier for Claude Desktop integration.
3. `napi-rs` FFI — link `pixors-state` directly into the Node runtime; no network overhead.

**Prerequisite for all three:** PR 2 must land first so `EditorState: Send`.
