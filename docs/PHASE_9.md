# PHASE 9 — Engine Architecture, Operations, Polish & Desktop Shell

> Audience: an implementer model. This document is prescriptive.
>
> Phase 9 has two tracks: **Engineering** (backend componentization, job system, operations)
> and **UX** (error surface, menu/panel cleanup, desktop shell). Engineering comes first — a clean
> backend makes the UX work trivial.

---

## Track A — Engineering: Componentization, Jobs, Operations

### A.1 Refactor `tab.rs` — Component-per-Service

**Current state:** `src/server/service/tab.rs` is ~1,200 lines. `TabData` owns everything: IO loading,
tile management, MIP generation, composition orchestration, layer mutation, and event emission.
Everything is coupled into one god object.

**Goal:** Split into standalone services. Each has its own `Command`/`Event` enum,
navigates the session state directly (`Session → Tab → Target`), and is the
**single authority** for its domain. One service per frontend component.

**Architecture:**

```
Session (HashMap<Uuid, TabData>)
  └── TabData
        ├── layers: Vec<LayerSlot>
        ├── viewports: HashMap<Uuid, ViewportState>
        └── jobs: Vec<Job>

TabService        ← TabCommand (CreateTab, CloseTab, ActivateTab)
                     → TabEvent (TabCreated, TabClosed, TabActivated)

LayerService      ← LayerCommand (SetVisible, SetOpacity, SetOffset, ...)
                     → LayerEvent (Changed, ...)
                     Path: Session → Tab → layers[find(layer_id)]

LoaderService     ← LoaderCommand (OpenFile { tab_id, path })
                     → LoaderEvent (ImageLoaded, ImageLoadProgress)
                     Path: Session → Tab → pipeline setup

ViewportService   ← ViewportCommand (Update, RequestTiles)
                     → ViewportEvent (Updated, TileReady, MipLevelReady)
                     Path: Session → Tab → viewports[tab_id]

JobService        ← JobCommand (Run, Cancel)
                     → JobEvent (Started, Progress, Done, Failed)
                     Path: Session → Tab → jobs

PreviewService    ← PreviewCommand (Start, Cancel)
                     → PreviewEvent (TileReady, Done)
                     Path: Session → Tab, reads from WorkingWriter at visible MIP

OperationService  ← OpCommand (Apply { tab_id, layer_id, op })
                     → OpEvent (Applied, Progress)
                     Path: Session → Tab → WorkingWriter
```

**Rules:**
- Each service has its **own** `Command` and `Event` enum — no shared parent enum
- Each service receives `&Arc<AppState>` and navigates: `state.session_manager → session → tab → field`
- Services **do not call each other**. They operate on shared state directly
- Events are emitted via `ctx.frame_tx` — no central dispatcher
- Frontend sends `ServiceName.CommandVariant` — each service handles only its commands
- `TabService` only does: create tab, close tab, activate tab, clear tab state. Nothing else.

**State ownership (in TabData after refactor):**
```rust
pub struct TabData {
    pub id: Uuid,
    pub name: String,
    pub layers: Vec<LayerSlot>,          // owned by LayerService
    pub viewports: HashMap<Uuid, Viewport>, // owned by ViewportService (keyed by tab_id)
    pub jobs: Vec<Job>,                  // owned by JobService
    pub doc_bounds: (u32, u32, i32, i32),
    // NOT in TabData: open_image pipeline (LoaderService owns it transiently)
}
```

**Service registration (app.rs):**
```rust
pub struct AppState {
    pub session_manager: SessionManager,
    pub tab_service:     Arc<TabService>,
    pub layer_service:   Arc<LayerService>,
    pub loader_service:  Arc<LoaderService>,
    pub viewport_service: Arc<ViewportService>,
    pub job_service:     Arc<JobService>,
    pub preview_service: Arc<PreviewService>,
    pub operation_service: Arc<OperationService>,
    pub event_tx:        broadcast::Sender<EngineEvent>,
}
```

**Command routing:**
```rust
// app.rs
pub async fn route_command(cmd: EngineCommand, state: &Arc<AppState>, ctx: &mut ConnectionContext) {
    match cmd {
        EngineCommand::Tab(c)       => state.tab_service.handle(c, state, ctx).await,
        EngineCommand::Layer(c)     => state.layer_service.handle(c, state, ctx).await,
        EngineCommand::Loader(c)    => state.loader_service.handle(c, state, ctx).await,
        EngineCommand::Viewport(c)  => state.viewport_service.handle(c, state, ctx).await,
        EngineCommand::Job(c)       => state.job_service.handle(c, state, ctx).await,
        EngineCommand::Preview(c)   => state.preview_service.handle(c, state, ctx).await,
        EngineCommand::Operation(c) => state.operation_service.handle(c, state, ctx).await,
    }
}
```

**Frontend 1-1 mapping:**
```
<MenuBar />       → TabService     (create/close/activate tabs)
<LayerPanel />    → LayerService   (visibility, opacity, offset)
<Viewport />      → ViewportService (zoom, pan, request tiles)
<Adjustments />   → OperationService (apply blur, contrast, etc.)
<StatusBar />     → JobService     (progress for open/save/apply)
```

### A.2 Job System

A `Job` is simply the execution of a pipeline — `source → pipes → sinks` — with progress tracking.

```rust
pub struct Job {
    pub id: Uuid,
    pub tab_id: Uuid,
    pub state: JobState,       // Pending, Running, Completed, Failed, Cancelled
    pub progress: f32,          // 0.0 .. 1.0
    pub total_tiles: u32,
    pub completed: AtomicU32,
}

pub enum JobState { Pending, Running, Completed, Failed(String), Cancelled }
```

A Job wraps any pipeline execution. Examples:
- `open_image` → source=Pipe, pipes=ColorConvert+MipPipe, sinks=Viewport+Working+Progress
- `apply_blur` → source=WorkingWriter MIP-0 tiles, pipe=BlurOp, sink=WorkingWriter
- `export_png` → source=CompositePipe at MIP-0, pipe=ColorConvert(f16→u8), sink=PNG writer

**Job events:**
```rust
JobStarted   { tab_id, job_id, total_tiles }
JobProgress  { tab_id, job_id, completed, total, percent }
JobDone      { tab_id, job_id }
JobFailed    { tab_id, job_id, error }
```

**Progress tracking:** The `ProgressSink` already counts tiles flowing through. Hook it to `Job::increment()`.

**Cancellation:** `Arc<AtomicBool>` flag. Every `Pipe` checks it at the top of its loop. `JobService::cancel(job_id)` sets the flag → pipes drain → `JobFailed(Cancelled)`.

### A.3 Preview — a Job constrained to the visible MIP level

A Preview is **exactly a Job**, but the pipeline operates only on tiles at the **current viewport MIP level**.
Same `JobState`, same progress events, same cancellation. The difference is scope: one MIP level vs all.

```rust
// Preview = Job with scope constraint
let preview_job = Job {
    kind: JobKind::Preview { op: Box::new(BlurOp(3)), mip_level: current_viewport_mip },
    ..
};
```

**Why single-MIP preview matters:** At zoom=0.02, MIP-5 ≈ 130×195 pixels → 1 tile.
Blur at MIP-5 is instant. Blur at MIP-0 is 425 tiles. User sees immediate feedback.

**Zoom tracking:** If the user zooms (MIP level changes), cancel the current preview Job,
start a new one at the new level. If the user clicks "Apply", promote to a full `OperationJob`
that runs on all MIP levels.

**Future:** Lock zoom during active preview — `ViewportService` rejects `ViewportUpdate`
while `PreviewService` has a running preview. See `ROADMAP.md`.

### A.4 First Operation — BLUR

```rust
/// Box filter blur with configurable radius (1–32).
/// Radius 1 = 3×3 kernel, radius 2 = 5×5, etc.
pub struct BlurOp {
    radius: u32,
}

impl Operation for BlurOp {
    fn name(&self) -> &str { "blur" }
    fn mip_aware(&self) -> bool { true }  // works correctly at any MIP level
    fn apply_tile(&self, src: &[Rgba<f16>], dst: &mut [Rgba<f16>], tile_size: u32, _params: &OpParams) -> Result<(), Error>;
}
```

**Implementation:** Separable box-filter: horizontal pass then vertical pass on a temporary buffer.
Simple, fast, no edge-case artifacts. Use `rayon` for large radii.

**Pipeline for preview:**
```
Frontend: { type: "preview", tab_id, layer_id, op: { type: "blur", radius: 3 } }
  → PreviewService:
      1. Cancel any existing preview for this tab/layer
      2. Create Preview { id, op: BlurOp(3), visible_mip: current_mip }
      3. For each tile at visible_mip:
         a. Read from WorkingWriter
         b. BlurOp::apply_tile(src, dst)
         c. Write to WorkingWriter
         d. Emit PreviewTileReady
      4. Emit PreviewDone
```

**Pipeline for apply (full Job):**
```
Frontend: { type: "run_job", tab_id, job: { kind: "apply_operation", ... } }
  → JobService:
      1. Create Job { kind: ApplyOperation { op: BlurOp(3), scope: ApplyScope::AllMips } }
      2. For each MIP level (0..max):
         For each tile:
           Read → Apply → Write → JobProgress
      3. Emit JobDone
```

---

## Track B — UX: Error Surface, Customizable UI, Desktop Shell

*(Original Phase 9 content follows, renumbered for clarity)*

---

## Scope

Two tracks, **Engineering first, then UX:**

**Track A — Engineering:**
1. Refactor `tab.rs` → component-per-service architecture
2. Job system with progress tracking and cancellation
3. Preview system (MIP-aware, per-zoom-level)
4. First operation: BLUR
5. Operation pipeline (tile read → apply → write → notify)

**Track B — UX:**
1. Error surface end-to-end
2. Menu cleanup
3. Panel cleanup
4. Customizable panels
5. Desktop shell (no Tauri)
6. Documentation pass for AI/MCP

Success bar per section. Engineering must be done before UX — a clean backend makes everything else trivial.

---

## B.1 Error Surface (end-to-end)

### Current state
- `Error` enum in `src/error.rs` is rich.
- Inside command handlers, errors are sometimes mapped to `EngineEvent::System(SystemEvent::Error { code, detail })`, sometimes logged-and-swallowed.
- Frontend `engineClient` does not de-mux errors: failures during a command in flight do not reject the corresponding promise.
- No tab-scoped error event; loading spinners can hang forever on a failed open.

### Backend tasks

#### 1.1 Single error funnel

**Current state:** `app.rs::route_command` does a flat `match` on `EngineCommand` → each service's
`handle_command(cmd, state, ctx)`. The `Service` trait returns `()` — no `Result`. Errors are
logged-and-swallowed or sent as ad-hoc `SystemEvent::Error`. No consistent error path.

**Goal:** Every command handler that can fail wraps its work and on error emits a typed error event
with a `req_id` so the frontend can correlate the error to the in-flight promise.

**Approach (no trait change):** Keep `handle_command` returning `()`. Each handler uses a helper:

```rust
async fn handle_with_error<T>(
    ctx: &mut ConnectionContext,
    req_id: Option<Uuid>,
    f: impl Future<Output = Result<T, Error>>,
) {
    match f.await {
        Ok(_) => {}
        Err(e) => {
            let code = ErrorCode::from(&e);
            send_session_event(&ctx.frame_tx, &EngineEvent::System(SystemEvent::Error {
                req_id, code, detail: Some(e.to_string()),
            }));
        }
    }
}
```

The `detail` field is `Option<String>` — the full error message for the frontend to display.
`ErrorCode` is machine-readable for branching logic.

#### 1.2 `ErrorCode` enum (new — `src/error/code.rs`)
Stable, machine-readable identifiers for the UI to branch on. Start small, grow as needed:
```rust
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    // I/O
    FileNotFound,
    FileUnreadable,
    UnsupportedFormat,
    UnsupportedColorSpace,
    DecodeFailed,
    // Tab / session
    TabNotFound,
    TabBusy,
    // Generic
    InvalidParameter,
    Internal,
}

impl From<&Error> for ErrorCode {
    fn from(e: &Error) -> Self {
        use Error as E;
        match e {
            E::Io(io) if io.kind() == std::io::ErrorKind::NotFound => Self::FileNotFound,
            E::Io(_)                       => Self::FileUnreadable,
            E::Png(_) | E::Tiff(_)         => Self::DecodeFailed,
            E::UnsupportedSampleType(_)
            | E::UnsupportedChannelLayout(_) => Self::UnsupportedFormat,
            E::UnsupportedColorSpace(_)    => Self::UnsupportedColorSpace,
            E::InvalidParameter(_)         => Self::InvalidParameter,
            _                              => Self::Internal,
        }
    }
}
```

#### 1.3 `req_id` already exists; make it mandatory on mutating commands
- Read commands (`get_*`, ping) — `req_id` optional.
- Mutating commands (`open_image`, `close_tab`, `apply_*`) — `req_id` required; reject with `InvalidParameter` if missing.

#### 1.4 Always emit `Ack`, even on success
Today only some handlers emit it. Make the dispatcher do it unconditionally. Handler bodies stop touching `Ack`.

### Frontend tasks

#### 1.5 `engineClient` keeps a `Map<req_id, { resolve, reject, timeoutId }>`
- `sendCommand(cmd)` returns `Promise<void>` that resolves on `Ack { status: ok }`, rejects on `Ack { status: error }` or `SystemEvent::Error { req_id }` matching this command.
- Default timeout: 30 s for open/save, 5 s for everything else; configurable per call.

#### 1.6 `<Toaster />` (Radix Toast)
- Add `@radix-ui/react-toast`.
- Component lives in `App.tsx` once.
- Subscribes to `SystemEvent::Error`. Shows: title = human label for `ErrorCode`, body = `detail`, action = "Copy details" (puts JSON on clipboard).

#### 1.7 Per-tab error surfacing
- `engine/store.ts` listens for `TabEvent::Error`. Sets `tab.loadError = { code, message }`, clears `tab.loading`.
- `Viewport.tsx` shows a centered error card when `activeTab.loadError` is set, with a "Retry" / "Close tab" button.

### Success bar
- Force a load failure (open `/dev/null` as `.png`) → toast appears within 200 ms, tab clears its spinner, no console errors, the in-flight promise rejects.
- Open a valid PNG, then a valid TIFF, then `/etc/passwd` (decode error), then a real PNG again. UI never gets stuck. Each error has a distinct toast.
- `cargo test` + `npm test` green.

---

## B.2 Menu Cleanup

### 2.1 Fix the hover-switch bug
`MenuBar.tsx` uses `@radix-ui/react-dropdown-menu`. `DropdownMenu` is **not** designed for menubar-style hover navigation; a dropdown does not yield focus to a sibling dropdown when the user mouses sideways.

**Fix**: switch to `@radix-ui/react-menubar` (different package, same family). It is purpose-built for this exact pattern: open one menu, hover sibling triggers → seamless transition.

```
npm install @radix-ui/react-menubar
```

Replace `DropdownMenu.Root/Trigger/Content/Item` with `Menubar.Menu/Trigger/Content/Item` in `MenuBar.tsx`. Wrap the row in `<Menubar.Root>`. CSS class names mostly translate one-to-one; spot-check the dropdown-content style.

### 2.2 Strip dead menu items
Current `MENU_ITEMS` ships ~50 entries; about 5 are wired. Replace with **only** what is implemented today. Anything not implemented does not appear. The Window menu is rebuilt in §4.

```ts
// src/components/MenuBar.tsx — new MENU_ITEMS
const MENU_ITEMS = [
  {
    label: 'File',
    items: [
      { label: 'Open…',  shortcut: 'Ctrl+O', action: () => engine.dispatch({ type: 'open_file_dialog' }) },
      { label: 'Close Tab', shortcut: 'Ctrl+W', action: closeActiveTab, enabled: hasActiveTab },
    ],
  },
  {
    label: 'View',
    items: [
      { label: 'Zoom In',         shortcut: 'Ctrl+=', action: () => viewportApi.zoomIn() },
      { label: 'Zoom Out',        shortcut: 'Ctrl+-', action: () => viewportApi.zoomOut() },
      { label: 'Fit to Screen',   shortcut: 'Ctrl+0', action: () => viewportApi.fit() },
      { label: 'Actual Size',     shortcut: 'Ctrl+1', action: () => viewportApi.actualSize() },
    ],
  },
  // Window menu defined in §4
  // Help intentionally omitted until there is a real "About" / docs link
];
```

Rules for inclusion:
- A menu entry must call a real engine command or a real client-side action that produces a visible effect.
- Entries that are documented future work go into `ROADMAP.md` instead. Not the menu.

### 2.3 Disabled state, not silent no-op
When `enabled` is false, render `<Menubar.Item disabled>`. Do not render an item that does nothing on click.

### 2.4 Keyboard shortcuts
Move the inline shortcut handler from `App.tsx::useKeymap` into a small `src/keymap.ts` exporting `registerShortcuts(actions: ShortcutMap)`. The menu definitions and the keymap consume the **same** `actions` map → no risk of menu and shortcut drifting.

### Success bar
- Open File menu, hover Edit/View → menu switches instantly (no click).
- Every menu item triggers a visible effect.
- No menu item is greyed-out for unrelated reasons (greyed = real precondition unmet, e.g. "Close Tab" with no tab open).

---

## B.3 Panel Cleanup

### 3.1 Audit & purge
Walk every panel in `Sidebar.tsx`:

| Panel | Today | Action |
|-------|-------|--------|
| Histogram | Fake random data (`Math.random()`) | **Delete**. Goes to `ROADMAP.md`. |
| Properties (W/H/X/Y) | Hardcoded `900/600/0/0` | **Delete**. Goes to `ROADMAP.md`. |
| Adjustments (Exposure, Contrast, …) | Sliders update local store only | **Delete**. Goes to `ROADMAP.md`. |
| Layers | Hard-coded `INIT_LAYERS` array | **Replace** with `activeTab.layers` from engine (read-only). Single source of truth. |

### 3.2 Single rule
Only Layers panel ships. It shows `activeTab.layers` — read-only. No mutations, no fake data.
Everything else goes to `ROADMAP.md`.

### 3.3 `uiStore` shrinks
Remove `Adjustments`, `Histogram`, `Properties`, all `INIT_*` arrays, all fake actions. Keep only:
- `mousePos`
- `panelLayout` (from §4)

### Success bar
- Layers panel shows engine layers truthfully (read-only).
- No panel renders fake data. `grep -RIn "INIT_\|Math.random" src/` → empty.

---

## B.4 Customizable Panels

### Goal
- Every panel can be **resized**.
- Every panel can be **redocked** to: `left`, `right`, `bottom`, `floating`, `hidden`.
- Layout **persists** in `localStorage`.
- A `Window` menu lists every panel as a checkbox (visible/hidden) plus a `Reset Layout` entry.

### 4.1 Choose the smallest library that fits

- **`react-resizable-panels`** for resizing within a region (left rail, right rail, bottom rail). ~6 KB gzipped, no docking, exactly what we need for the resize part.
- **No drag-and-drop framework.** Redocking is a click action via the panel's header context menu (`Move to → Left / Right / Bottom / Float`). This sidesteps the entire dnd-kit ecosystem and is faster to use anyway.

### 4.2 Data model (single source of truth)

```ts
// src/ui/panelLayout.ts
export type PanelId = 'layers';
export type Slot    = 'left' | 'right' | 'bottom' | 'float' | 'hidden';

export interface PanelState {
  id: PanelId;
  slot: Slot;
  size: number;              // px when docked, ignored when float/hidden
  order: number;             // within the slot (top-to-bottom or left-to-right)
  floatRect?: { x: number; y: number; w: number; h: number };
}

export interface PanelLayout {
  version: 1;
  panels: Record<PanelId, PanelState>;
  rails: { left: number; right: number; bottom: number };  // rail widths/height
}

export const DEFAULT_LAYOUT: PanelLayout = { /* ... */ };
```

Persistence: zustand `persist` middleware → `localStorage` key `pixors.panelLayout.v1`. On load, if `version` mismatches stored data, fall back to `DEFAULT_LAYOUT` (silent migration; do not crash on stale layouts).

### 4.3 Workspace structure (`App.tsx`)

```
<MenuBar />
<TabBar />
<PanelGroup direction="horizontal">
  <Panel slot="left"   />        {/* react-resizable-panels */}
  <PanelGroup direction="vertical">
    <Panel>                       {/* viewport */}
      <Viewport />
    </Panel>
    <Panel slot="bottom" />
  </PanelGroup>
  <Panel slot="right"  />
</PanelGroup>
<FloatingPanelLayer />            {/* renders Slot=float panels */}
<StatusBar />
```

Each `Panel slot=…` renders the ordered list of `PanelState` whose slot matches. A floating panel is a draggable window (use `pointerdown` + `pointermove` — no library).

### 4.4 Per-panel header

```
[≡] Properties                           [□ Float] [✕ Hide]
```

`[≡]` opens a small menu: "Move to → Left | Right | Bottom | Float". `[□]` toggles float ↔ last-docked-slot. `[✕]` sets `slot=hidden`. Re-open from the Window menu.

### 4.5 `Window` menu (re-introduced in §2.2)

```ts
{
  label: 'Window',
  items: [
    ...Object.values(layout.panels).map(p => ({
      label: prettyName(p.id),
      checked: p.slot !== 'hidden',
      action: () => togglePanelVisibility(p.id),
    })),
    { type: 'separator' },
    { label: 'Reset Layout', action: () => setLayout(DEFAULT_LAYOUT) },
  ],
}
```

Use `Menubar.CheckboxItem` for the toggles.

### 4.6 Boundaries
- Resize: snap to 8 px multiples; minimum panel size 120 px.
- Float: clamp inside the workspace; minimum 200×120; remember position per panel.
- Hidden panels keep their last `size` and `slot` so re-showing restores them in place.

### Success bar
- Resize a rail → page reloads with the same widths.
- Move "Layers" from right to left → reload preserves it.
- Float "Properties", drag it around → reload restores its rect.
- Hide every panel → viewport fills the workspace; Window menu shows everything unchecked; "Reset Layout" restores defaults instantly.

---

## B.5 Desktop Shell (no Tauri)

### Why not Tauri
- Tauri assumes the app is the IPC layer between web and Rust. Our engine is already a WebSocket service — we would be adding an alternate command channel that we then have to keep in sync with the WS protocol. Two protocols for one app.
- Native menus / tray are out of scope (the in-app menu bar from §2 is the menu).
- Tauri pulls a large build/sign chain (signing identities, updater, plugin system) that we do not need yet.
- The engine **must stay deployable as a headless server** for MCP and (later) mobile. Bolting Tauri on creates a second binary shape to maintain.

### The chosen architecture

A single Rust binary that:
1. Starts the existing engine WebSocket server on `127.0.0.1:<random free port>`.
2. Serves the built UI bundle (output of `vite build`) over HTTP from the same port (a new tiny `axum` route under `/`).
3. Opens a system webview pointed at `http://127.0.0.1:<port>`.

When the engine is run with `--headless` (or no UI bundle is embedded), step 2/3 are skipped → the same binary is the MCP/server build.

### 5.1 Embed the UI bundle

`pixors-engine/build.rs`: if env var `PIXORS_BUNDLE_UI=1`, run `npm --prefix ../pixors-ui ci && npm --prefix ../pixors-ui run build`, then embed the `dist/` directory via `include_dir = "0.7"` into a static `UI_DIST: Dir<'static>`.

`pixors-engine/src/server/ui_routes.rs`:
```rust
pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/",    get(serve_index))
        .route("/*p",  get(serve_asset))
}
```
`serve_index` returns `index.html` with `<base href="/">` rewriting if needed. `serve_asset` looks up `UI_DIST.get_file(path)` and falls back to `index.html` (SPA fallback) for unknown paths.

The route is **only mounted when** `cfg!(feature = "ui-bundle")` and the bundle is non-empty. Headless build does not pay the binary-size cost.

### 5.2 Webview window — `wry` only, no Tauri

`wry` is the same webview crate Tauri uses internally, on its own. ~25 kloc, one dependency, BSD-style license. It opens a native window with the system webview (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux). It does **not** impose a project structure.

```toml
# pixors-engine/Cargo.toml — new optional dep
[features]
desktop   = ["wry", "tao", "ui-bundle"]
ui-bundle = ["include_dir"]
mcp       = []   # placeholder; documented in §6

[dependencies]
wry        = { version = "0.45", optional = true }
tao        = { version = "0.30", optional = true }   # window mgmt for wry
include_dir = { version = "0.7", optional = true }
```

`src/bin/pixors-desktop.rs` (new):
```rust
fn main() -> anyhow::Result<()> {
    let port = pick_free_port()?;
    let engine_handle = pixors_engine::server::start(port)?;  // existing API

    let event_loop = tao::event_loop::EventLoop::new();
    let window = tao::window::WindowBuilder::new()
        .with_title("Pixors")
        .with_min_inner_size(tao::dpi::LogicalSize::new(1024, 640))
        .build(&event_loop)?;

    let _webview = wry::WebViewBuilder::new(&window)
        .with_url(&format!("http://127.0.0.1:{port}"))?
        .build()?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = tao::event_loop::ControlFlow::Wait;
        if let tao::event::Event::WindowEvent {
            event: tao::event::WindowEvent::CloseRequested, ..
        } = event {
            engine_handle.shutdown_blocking();
            *control_flow = tao::event_loop::ControlFlow::Exit;
        }
    });
}
```

That is the entire desktop shell: ~40 lines.

### 5.3 Headless / MCP-friendly mode
`src/bin/pixors-server.rs` exists already (or is the current `main.rs` minus the desktop bits). It just runs `pixors_engine::server::start(port)` and blocks. This binary has zero UI/wry dependencies and is what MCP and mobile clients will speak to.

`Cargo.toml`:
```toml
[[bin]]
name = "pixors-server"   # headless, default features
path = "src/bin/pixors-server.rs"

[[bin]]
name = "pixors-desktop"  # only built with --features desktop
path = "src/bin/pixors-desktop.rs"
required-features = ["desktop"]
```

### 5.4 Build artefacts
- `cargo build --release --bin pixors-server` → headless binary, runs anywhere.
- `PIXORS_BUNDLE_UI=1 cargo build --release --features desktop --bin pixors-desktop` → single executable with embedded UI; double-click to run.
- No installer for now; ship the raw binary per platform. Code-signing is a future concern.

### 5.5 Window/tab close
- App-level: window-close stops the engine via the existing `graceful shutdown` path (already needed for §1).
- Tab-level: the X on a tab dispatches `close_tab { tab_id }` which the engine answers with `TabEvent::Closed`. Ack pattern from §1 covers it.

### Success bar
- `cargo build --release --bin pixors-server` runs anywhere, headless. Frontend opens by browser pointing at the printed URL.
- `cargo build --release --features desktop --bin pixors-desktop` → one binary; running it opens a window with the editor; closing the window shuts down the engine cleanly (no zombie process).
- The headless binary has **zero** mention of `wry`/`tao`/`include_dir` symbols (`cargo bloat` to verify if needed).

---

## B.6 AI / MCP Documentation Pass

The engine already speaks a clean WS protocol. To make it pleasant for an LLM (via MCP) to drive, we ship three small docs in `docs/`:

### 6.1 `PROTOCOL.md`
Full enumeration of every `EngineCommand` and `EngineEvent`, with:
- `req_id` semantics (mandatory on mutations, see §1.3).
- Per-command JSON example and expected event sequence.
- `ErrorCode` table from §1.2 with one-sentence remediation per code.
- A "Minimum viable session" section: `connect → open_image → wait Tab::Loaded → query_pixels → apply_op → wait Ack`.

This file is the source of truth; the WS handler points to it in code comments.

### 6.2 `MCP_INTEGRATION.md`
- How to wrap `pixors-server` as an MCP tool. (Stub: the MCP server crate lives outside this repo for now; this doc names the tool surface.)
- Table of MCP tool names → underlying `EngineCommand`.
- Why we send tile-level deltas (so the LLM never has to ship the whole image back).
- Example transcripts: "Crop and brighten image at /home/.../foo.png" → tool calls.

### 6.3 `ROADMAP.md`
Everything that was deleted from the menus/panels in §2/§3 lands here, plus any future ideas
across the entire stack (frontend and backend):
- One-line description.
- Which engine capability it depends on.
- Suggested phase to revisit.

This keeps the implementer (human or AI) honest: deletions are tracked, not lost.

### 6.4 Update `CLAUDE.md`
Add a "Phase 9 — Polish, Errors, Customization, Desktop" entry. Replace any "Tauri" mention. Add a one-paragraph note that the engine is deliberately framework-agnostic for MCP/mobile reuse.

### Success bar
- A new contributor (or LLM) can read `PROTOCOL.md` and write a working WS client in one sitting.
- `MCP_INTEGRATION.md` lists every tool surface needed; no hidden state.
- `ROADMAP.md` has at least the entries deleted in §2.2 and §3.1.

---

## Migration Order

**Track A (Engineering) — sequential:**
1. **A.1** Refactor `tab.rs` → component-per-service (largest change, unblocks everything)
2. **A.2** Job system + progress tracking (hook to existing `ProgressSink`)
3. **A.4** BLUR operation + `Operation` trait
4. **A.3** Preview system (MIP-aware, reuses Job + Operation)
5. **A.5** Verify WorkingWriter MIP path (should already work)

**Track B (UX) — sequential, after Engineering:**
6. **B.1** backend funnel + ErrorCode + ack everywhere
7. **B.1** frontend toaster + per-tab error state
8. **B.2** menubar fix (drop-in package swap)
9. **B.2** strip dead menu items (delete-only)
10. **B.3** panel purge + uiStore shrink (delete-only)
11. **B.4** panel layout model + persistence + Window menu
12. **B.5** desktop binary (additive Cargo features)
13. **B.6** docs

Each step ships independently. Reviewer checks the success bar before merging.

---

## What we explicitly do NOT do

- **No Tauri, no Electron, no Neutralino.** Single binary + `wry` window + the existing WS protocol. Engine stays MCP-deployable.
- **No native OS menus.** The in-app `Menubar` from §2 is the only menu surface. Identical UX on every OS.
- **No drag-and-drop panel docking.** Click → "Move to → …" is enough and avoids dnd-kit.
- **No new IPC channel for desktop.** The webview talks to the engine over `ws://127.0.0.1:<port>` exactly like the dev browser does.
- **No code-signing pipeline this phase.** Future concern.
- **No new domain abstractions in the engine for the desktop shell.** The shell is a `pixors-engine/src/bin/*.rs` plus a feature-gated route module. That is the entirety of the engine-side change.
- **No fake UI state.** Anything that does not reflect engine truth is deleted, not stubbed.

---

## Definition of Done (Phase 9)

**Track A:**
- `tab.rs` split into ≤ 6 sub-services, each ≤ 300 lines
- `JobService` tracks open, apply, export, mip-gen with real-time progress events
- `PreviewService` applies BLUR to visible MIP level in < 100ms for any zoom level
- `Operation` trait implemented for `BlurOp` with mip_aware=true
- `cargo test --lib` green, no regressions

**Track B:**
- All five success bars (B.1–B.5) met
- `cargo build --release --bin pixors-server` headless; `--bin pixors-desktop` double-clickable
- Menu/panel surface contains only wired-up actions
- Force-failure scenarios surface as toasts
- Panel layout survives full app restart
- `docs/PROTOCOL.md`, `docs/MCP_INTEGRATION.md`, `ROADMAP.md` exist
