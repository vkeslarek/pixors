# Phase 6: Housekeeping, Tile-Aware Engine & Functional UI

## Overview

Phase 6 shifts from visual scaffolding to **real functionality**. The engine gets a deep refactor
around tile-aware, lazy-loaded storage with clear resource ownership. The UI gains working tabs,
file opening, and professional viewport navigation. The MCP bridge is updated to match the new
tab-based protocol and actually tested for the first time.

**Guiding principles:**

1. **Tile-Aware everything** — every pipeline stage operates on tiles, never full images in RAM.
2. **Lazy promotion** — data lives on disk until needed; only tiles visible in the viewport are
   decoded to RAM. Two tiers for now: **Disk** → **RAM**.
3. **Clear ownership** — closing a tab destroys its `Tab`, which cascades to every resource
   (tiles, MIP cache, WebSocket state). No leaks.
4. **SIMD everywhere** — color conversion, MIP generation, and tile extraction use `std::simd`
   (or `packed_simd2` / manual intrinsics) for ≥4× throughput.

---

## 1. Engine: Tile-Aware Storage Refactor

### 1.1 Separate Storage from Loading

**Current problem:** `FileService::open_image()` decodes the entire image into a monolithic
`TypedImage<Rgba<f16>>` in RAM, then `TileGrid` creates zero-copy tile views over it. This means
the full image must fit in RAM even if only a small region is visible.

**Target architecture:**

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│  ImageSource │────▶│  TileStore   │────▶│  TileCache   │
│  (decoder)   │     │  (disk tier) │     │  (RAM tier)  │
└─────────────┘     └──────────────┘     └──────────────┘
      │                    │                     │
  Reads raw file     Stores decoded f16     LRU eviction,
  on demand          tiles to temp dir      serves viewport
```

**Files to create/modify:**

| File | Action | Description |
|------|--------|-------------|
| `src/storage/mod.rs` | **Create** | Module root, re-exports |
| `src/storage/source.rs` | **Create** | `ImageSource` trait — async tile-level decoder |
| `src/storage/png_source.rs` | **Create** | PNG implementation of `ImageSource` |
| `src/storage/tile_store.rs` | **Create** | Disk-backed tile storage (temp dir per tab) |
| `src/storage/tile_cache.rs` | **Create** | LRU RAM cache for hot tiles |
| `src/image/tile.rs` | **Modify** | `TileGrid` becomes metadata-only (no `Arc<TypedImage>`) |
| `src/server/service/file.rs` | **Modify** | Use `ImageSource` instead of loading full image |

**`ImageSource` trait:**

```rust
/// Async, tile-level image decoder. Implementations are format-specific.
#[async_trait]
pub trait ImageSource: Send + Sync {
    /// Image dimensions (available after open, before any tile decode).
    fn dimensions(&self) -> (u32, u32);

    /// Decode a single tile region to ACEScg premul f16.
    /// The implementation may read only the relevant bytes from disk.
    async fn decode_tile(&self, x: u32, y: u32, w: u32, h: u32)
        -> Result<Vec<Rgba<f16>>, Error>;
}
```

**`TileStore` (disk tier):**

```rust
/// Persists decoded tiles as raw f16 blobs in a temp directory.
/// Each tile is a file: `{tab_tmp}/{tile_x}_{tile_y}.raw`
pub struct TileStore {
    base_dir: PathBuf,           // e.g. /tmp/pixors-{tab_id}/
    tile_size: u32,
    image_width: u32,
    image_height: u32,
}

impl TileStore {
    /// Writes a decoded tile to disk. Called lazily on first access.
    pub async fn put(&self, tile: &Tile, data: &[Rgba<f16>]) -> Result<(), Error>;

    /// Reads a tile from disk. Returns None if not yet decoded.
    pub async fn get(&self, tile: &Tile) -> Result<Option<Vec<Rgba<f16>>>, Error>;

    /// Checks if a tile has been decoded and stored.
    pub fn has(&self, tile: &Tile) -> bool;

    /// Deletes ALL files (called on tab close).
    pub fn destroy(&self) -> Result<(), Error>;
}
```

**`TileCache` (RAM tier — LRU):**

```rust
/// In-memory LRU cache for tiles that are actively needed by the viewport.
/// Capacity is in number of tiles (e.g., 256 tiles × 256×256×8 bytes ≈ 128 MB).
pub struct TileCache {
    cache: RwLock<LruCache<TileKey, Arc<Vec<Rgba<f16>>>>>,
    max_tiles: usize,
}

impl TileCache {
    /// Get a tile, promoting from TileStore → RAM if needed.
    pub async fn get_or_load(
        &self,
        tile: &Tile,
        store: &TileStore,
        source: &dyn ImageSource,
    ) -> Result<Arc<Vec<Rgba<f16>>>, Error>;

    /// Evict all tiles for a tab.
    pub fn evict_tab(&self, tab_id: &Uuid);
}
```

### 1.2 Lazy Tile Decoding Flow

When a viewport requests tiles:

1. Check `TileCache` (RAM) — if hit, return immediately.
2. Check `TileStore` (disk) — if hit, load into cache, return.
3. Call `ImageSource::decode_tile()` — decode from original file, write to `TileStore`,
   insert into `TileCache`, return.

Only tiles **visible in the current viewport** are decoded. Pan/zoom triggers new tile requests.

### 1.3 Resource Ownership & Tab Lifecycle

```
Tab (UUID)
├── ImageSource (holds file handle)
├── TileStore (owns temp directory)
├── TileCache entries (keyed by tab)
├── MipPyramid (per-image)
└── ViewportState (pan/zoom/size)
```

**Closing a tab must:**

1. `DELETE /api/tab/:id` from frontend
2. Engine calls `TabService::delete_tab(id)`:
   - Drops `ImageSource` (closes file handle)
   - Calls `TileStore::destroy()` (deletes temp files)
   - Calls `TileCache::evict_tab(id)` (frees RAM)
   - Removes `ViewportState`
3. Frontend removes tab from state

**Implementation in `TabState`:**

```rust
pub struct TabState {
    pub id: Uuid,
    pub created_at: u64,
    pub source: Option<Box<dyn ImageSource>>,
    pub tile_store: Option<TileStore>,
    pub mip_pyramid: Option<MipPyramid>,
    pub tile_size: u32,
    // image dimensions cached from source
    pub width: u32,
    pub height: u32,
}

impl Drop for TabState {
    fn drop(&mut self) {
        // TileStore::destroy() is called here to clean temp files
        if let Some(store) = self.tile_store.take() {
            let _ = store.destroy();
        }
    }
}
```

---

## 2. MIP Pyramid for Smooth Zoom

### 2.1 Why MIPs

When zoomed out on a 50 MP image, sampling every 20th pixel creates aliasing. A MIP pyramid
provides pre-filtered downscaled versions for each zoom level, giving smooth results.

### 2.2 Structure

```rust
/// Pre-computed downscaled versions of the image for fast zoom-out rendering.
pub struct MipPyramid {
    /// Level 0 = full resolution (not stored, use TileStore).
    /// Level 1 = 1/2 resolution, Level 2 = 1/4, etc.
    /// Each level is itself tile-aware.
    levels: Vec<MipLevel>,
}

pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub scale: f32,        // 0.5, 0.25, 0.125, ...
    pub store: TileStore,  // each MIP level has its own tile store
}
```

### 2.3 Generation (SIMD)

MIP generation uses a 2×2 box filter with SIMD:

```rust
/// Generate MIP level N+1 from level N using SIMD 2×2 box filter.
/// Processes 4 pixels (16 f16 channels = 32 bytes) per SIMD iteration.
pub fn generate_mip_level_simd(
    src_tiles: &TileStore,
    dst_store: &mut TileStore,
    src_width: u32,
    src_height: u32,
    tile_size: u32,
) -> Result<(u32, u32), Error>;
```

- Use `std::arch::x86_64` with `_mm256` intrinsics for AVX2, fallback to scalar.
- Process 4 RGBA f16 pixels at once: load 2×2 block → widen to f32 → average → narrow to f16.
- Generate levels lazily: only compute a MIP level when the viewport zoom requires it.
- Stop at 1×1 tile (typically 7–8 levels for a 50 MP image).

### 2.4 Zoom-Level Selection

```rust
/// Select the appropriate MIP level for the current zoom.
/// Returns 0 for zoom >= 0.5, 1 for zoom >= 0.25, etc.
pub fn mip_level_for_zoom(zoom: f32) -> usize {
    if zoom >= 0.5 { return 0; }
    (-(zoom.log2())).floor() as usize
}
```

---

## 3. SIMD Color Conversion

### 3.1 Current State

`convert::convert_acescg_premul_region_to_srgb_u8()` processes pixels one at a time with scalar
f16→f32→matrix→gamma→u8. This is the bottleneck for tile streaming.

### 3.2 Target

Process **8 pixels per iteration** using AVX2 (or 4 with SSE4.1):

```rust
/// Convert 8 ACEScg premul f16 pixels to sRGB u8 in one pass.
/// Uses AVX2: f16×8 → f32×8 → matrix 3×3 → sRGB gamma → u8×8
#[target_feature(enable = "avx2", enable = "f16c")]
unsafe fn convert_8px_acescg_to_srgb(
    src: &[Rgba<f16>; 8],
    dst: &mut [u8; 32],  // 8 pixels × 4 channels
    matrix: &[f32; 9],
);
```

**Steps per 8-pixel batch:**
1. Load 8× RGBA f16 (64 bytes) → `_mm256_cvtph_ps` → 8× f32
2. Unpremultiply alpha (divide RGB by A, guard against A=0)
3. Apply 3×3 color matrix (ACEScg → sRGB primaries) via FMA
4. Apply sRGB gamma curve: `x ≤ 0.0031308 ? 12.92*x : 1.055*x^(1/2.4) - 0.055`
   (use polynomial approximation for `pow` — `fast_srgb_gamma_approx`)
5. Clamp [0, 255] → `_mm256_cvtps_epi32` → pack to u8
6. Re-apply premul if container requires it (PNG: straight alpha, so skip)

**Fallback:** scalar path for non-x86 or missing feature flags, gated by `#[cfg]`.

---

## 4. Event-Driven Architecture

### 4.1 Core Principle: Engine as Single Source of Truth

The engine is the **only** authority on application state. Neither the UI nor the MCP ever
mutate local state directly. Instead:

```
┌─────────┐  Command   ┌────────────┐  Event    ┌─────────┐
│   UI    │───────────▶│   Engine   │──────────▶│   UI    │
│ (React) │            │  (Axum)    │           │ (React) │
└─────────┘            └────────────┘           └─────────┘
                            ▲  │
┌─────────┐  Command        │  │  Event    ┌─────────┐
│   MCP   │─────────────────┘  └──────────▶│   MCP   │
│ (stdio) │                                │ (stdio) │
└─────────┘                                └─────────┘
```

**Commands** (client → engine): "I want X to happen" — may be rejected.
**Events** (engine → all clients): "X happened" — broadcast to every connected client.

This means the MCP can open files, switch tabs, select tools, and the UI reacts exactly as if
the user did it. Conversely, user actions in the UI go through the same command path.

### 4.2 Event Types

```rust
#[derive(Serialize)]
#[serde(tag = "type")]
pub enum EngineEvent {
    // Tab lifecycle
    TabCreated     { tab_id: Uuid, name: String },
    TabClosed      { tab_id: Uuid },
    TabActivated   { tab_id: Uuid },

    // Image lifecycle
    ImageLoaded    { tab_id: Uuid, width: u32, height: u32, format: PixelFormat },
    ImageClosed    { tab_id: Uuid },

    // Tile streaming (per-tab WS only)
    TileData       { x: u32, y: u32, width: u32, height: u32, size: usize },
    TilesComplete,
    TilesDirty     { tab_id: Uuid, regions: Vec<TileRect> },

    // Tool / UI state
    ToolChanged    { tool: String },
    ViewportUpdated { tab_id: Uuid, zoom: f32, pan_x: f32, pan_y: f32 },

    // Errors
    Error          { message: String },
}
```

### 4.3 Command Types

```rust
#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum EngineCommand {
    CreateTab,
    CloseTab       { tab_id: Uuid },
    ActivateTab    { tab_id: Uuid },
    OpenFile       { tab_id: Uuid, path: String },
    ViewportUpdate { x: f32, y: f32, w: f32, h: f32, zoom: f32 },
    SelectTool     { tool: String },
    GetState,
    Screenshot,
    Close,
}
```

### 4.4 Unidirectional Flow in the UI

The React UI **never** calls `setState` directly from user input:

```typescript
// ❌ WRONG — direct mutation
const handleOpenFile = (path: string) => {
  setTabs([...tabs, { id: newId, name: path }])
}

// ✅ CORRECT — command → event round-trip
const handleOpenFile = (path: string) => {
  ws.send(JSON.stringify({ type: "open_file", tab_id: activeTabId, path }))
  // State updates ONLY on receiving TabCreated + ImageLoaded events
}

// Central WS event handler:
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data)
  switch (msg.type) {
    case "tab_created":
      setTabs(prev => [...prev, { id: msg.tab_id, name: msg.name }])
      break
    case "tab_closed":
      setTabs(prev => prev.filter(t => t.id !== msg.tab_id))
      break
    case "tool_changed":
      setActiveTool(msg.tool)
      break
  }
}
```

### 4.5 Bidirectional Tile Flow

**Pull (viewport → engine):** Viewport sends `viewport_update`. Engine streams visible tiles back.

**Push (engine → viewport):** An edit (from UI or MCP) modifies pixels → engine broadcasts
`TilesDirty { regions }` → viewport re-requests dirty tiles.

```
Pan/Zoom:  UI ──viewport_update──▶ Engine ──tile_data──▶ UI
Edit:      MCP ──apply_filter───▶ Engine ──tiles_dirty─▶ UI ──viewport_update──▶ Engine ──tile_data──▶ UI
```

### 4.6 MCP as Full Remote Controller

| MCP Tool | Command | Effect |
|----------|---------|--------|
| `pixors_create_tab` | `CreateTab` | Opens a new empty tab |
| `pixors_open_image` | `OpenFile` | Loads file into a tab |
| `pixors_close_tab` | `CloseTab` | Closes tab, frees resources |
| `pixors_activate_tab` | `ActivateTab` | Switches active tab (UI updates) |
| `pixors_select_tool` | `SelectTool` | Changes active tool |
| `pixors_get_state` | `GetState` | Full app state snapshot |
| `pixors_screenshot` | `Screenshot` | Viewport capture as base64 PNG |

### 4.7 Global Event Bus (Engine-Side)

```rust
pub struct EventBus {
    subscribers: RwLock<Vec<mpsc::UnboundedSender<EngineEvent>>>,
}

impl EventBus {
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<EngineEvent>;
    pub async fn broadcast(&self, event: EngineEvent);
    pub async fn broadcast_to_tab(&self, tab_id: &Uuid, event: EngineEvent);
}
```

Each WS connection spawns two tasks:
1. **Reader**: receives `EngineCommand`, dispatches to engine
2. **Writer**: receives `EngineEvent` from `EventBus`, sends to client

---

## 5. API Simplification

### 5.1 REST API

| Method | Path | Body | Response | Description |
|--------|------|------|----------|-------------|
| `POST` | `/api/tabs` | `{}` | `{ tab_id }` | Create tab |
| `DELETE` | `/api/tabs/:id` | — | `204` | Destroy tab + all resources |
| `POST` | `/api/tabs/:id/open` | `{ path }` | `{ width, height, tile_count }` | Open file |
| `GET` | `/api/tabs/:id/info` | — | `{ width, height, has_image }` | Tab info |
| `GET` | `/api/state` | — | Full app state | For MCP |

### 5.2 WebSocket

Per-tab connection: `ws://host/ws?tab_id=UUID`. Uses `EngineCommand`/`EngineEvent` (§4.2–4.3).

---

## 6. Frontend: Working Tabs & File Opening

### 6.1 Tab Lifecycle (Event-Driven)

```
User opens file → send CreateTab → receive TabCreated → send OpenFile → receive ImageLoaded
User clicks tab → send ActivateTab → receive TabActivated → switch active
User closes tab → send CloseTab → receive TabClosed → remove from state
MCP opens file → same commands → same events → UI updates automatically
```

### 6.2 Files to Modify

| File | Changes |
|------|---------|
| `App.tsx` | Event-driven tab state, Ctrl+O handler |
| `MenuBar.tsx` | Wire File → Open to file dialog |
| `Viewport.tsx` | Accept `tabId` prop, reconnect WS on change |
| `types.ts` | Add `tabId`, `path` to `Tab` |
| `hooks/useEngineEvents.ts` | **Create** — central WS event dispatcher |

### 6.3 Viewport Tab Switching

```typescript
interface ViewportProps {
  tabId: string | null   // null → empty state
  activeTool: string
  zoom: number
  onMouseMove: (x: number, y: number) => void
}
```

---

## 7. Viewport UX: Pan, Zoom & Pixel Grid

### 7.1 Navigation (Blender-Style)

| Action | Input |
|--------|-------|
| Pan | Middle mouse drag / Ctrl+Left drag / Space+Left drag |
| Zoom | Scroll wheel (up = in) / Ctrl+Scroll |
| Fit | Home / double-click middle |
| 100% | Numpad 1 / Ctrl+1 |

### 7.2 Pixel Grid (Shader-Based)

At >800% zoom, add 1px grid lines in `shader.wgsl`. Requires `image_size` in `CameraUniform`:

```rust
pub struct CameraUniform {
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub image_size: [f32; 2],  // NEW
    pub _pad: [f32; 2],
}
```

---

## 8. MCP Bridge Update (`pixors-mcp`)

Update to tab-based REST + WS event bus. Expose all tools from §4.6. Add `Screenshot` for
MCP to "see" the viewport. Test with stdio pipe against running engine.

---

## 9. Implementation Order

### Step 1: Event Bus (Engine)
- Create `event_bus.rs`, define `EngineEvent` + `EngineCommand`
- Wire into `AppState`, update WS handler with reader+writer tasks
- **Test:** two WS clients, command from one → both receive event

### Step 2: API Routes (Engine)
- `/api/tabs` (plural), `:id/open`, `/api/state`
- Commands go through EventBus
- **Test:** `cargo test` + `cargo check` clean

### Step 3: Storage Module (Engine)
- `src/storage/`: `ImageSource`, `PngSource`, `TileStore`, `TileCache`
- **Test:** store/cache round-trip unit tests

### Step 4: Tab Refactor (Engine)
- `TabState` owns `ImageSource` + `TileStore` + `TileCache`, `Drop` cascades
- **Test:** create → open → delete → verify temp files gone

### Step 5: SIMD Color Conversion (Engine)
- AVX2 path in `convert/`, scalar fallback
- **Test:** output parity + benchmark ≥3×

### Step 6: MIP Pyramid (Engine)
- `MipPyramid`, lazy generation, select level from zoom
- **Test:** verify MIP dimensions at each level

### Step 7: Viewport Tile Streaming (Engine)
- `viewport_update` handler, `sent_tiles` tracking, `tiles_dirty` push
- **Test:** WS → viewport_update → correct tiles received

### Step 8: MCP Update (`pixors-mcp`)
- Tab-based protocol, all tools, stdio test
- **Test:** create → open → screenshot → close

### Step 9: Frontend Event-Driven Tabs (`pixors-ui`)
- `useEngineEvents.ts` hook, event-driven state, file dialog
- **Test:** MCP opens file → tab appears in UI

### Step 10: Viewport Navigation (`pixors-ui`)
- Middle mouse pan, scroll zoom, Home/Ctrl+1 shortcuts

### Step 11: Pixel Grid (`pixors-viewport`)
- `image_size` uniform, shader grid overlay, rebuild WASM

---

## 9.1 Execution Status (2026-04-23)

Use this section as a handoff checklist for another model/agent.

### Already implemented

- [x] **Step 1 – Event Bus (engine):** `EngineEvent`/`EngineCommand` created, WS reader/writer tasks wired.
- [x] **Step 2 – API Routes (engine):** `/api/tabs`, `/api/tabs/:id/open`, `/api/state` implemented.
- [x] **Step 3 – Storage Module (engine):** `ImageSource`, `PngSource`, `TileStore`, `TileCache` exist.
- [x] **Step 4 – Tab refactor (engine):** `TabState` owns source/store/grid and cleanup on drop.
- [~] **Step 5 – SIMD color conversion (engine):** SIMD x4 matrix path integrated in conversion hot path (`wide::f32x4`), but AVX2/f16c x8 path + benchmark target still pending.
- [~] **Step 6 – MIP Pyramid (engine):** metadata structures added (`MipPyramid`, `MipLevel`, `mip_level_for_zoom`) and attached to tab state; lazy tile-store-backed mip generation is still pending.
- [~] **Step 7 – Viewport tile streaming (engine):** `viewport_update` streams visible tiles, `sent_tiles` dedupe added, WS tile transfer switched to JSON metadata + binary payload. `tiles_dirty` invalidation flow still incomplete.
- [ ] **Step 8 – MCP Update (`pixors-mcp`):** not implemented yet.
- [x] **Step 9 – Frontend event-driven tabs (`pixors-ui`):** central event hook + command/event round-trip for tab lifecycle and open file.
- [x] **Step 10 – Viewport navigation (`pixors-ui`):** pan/zoom + fit/100% shortcuts implemented.
- [ ] **Step 11 – Pixel grid (`pixors-viewport`):** not implemented yet.

---

## 9.2 Detailed Handoff – Step 6 (MIP Pyramid)

> **User priority note:** The “integrate mip level selection in runtime viewport path” subtask was explicitly deprioritized in this handoff. Focus on lazy generation and MCP work first.

### What is already done

- `pixors-engine/src/image/mip.rs` exists with:
  - `MipPyramid` metadata builder,
  - `MipLevel` structure,
  - `mip_level_for_zoom(zoom)` helper,
  - placeholder `generate_mip_level_simd(src_width, src_height)` dimension reducer.
- `pixors-engine/src/image/mod.rs` exports the mip module.
- `pixors-engine/src/server/service/tab.rs` stores `mip_pyramid: Option<MipPyramid>` and initializes it on image open.

### What still needs to be implemented (for another model)

- [ ] **Implement real lazy mip generation backed by tile stores** (not only metadata):
  - create one `TileStore` per mip level,
  - generate level `N+1` only when first needed,
  - persist generated level tiles to disk tier, cache in RAM via `TileCache`.
- [ ] **Implement tile-level 2×2 box filtering from source tiles**:
  - correct border handling for odd dimensions,
  - deterministic and testable output.
- [ ] **Replace placeholder generator**:
  - current `generate_mip_level_simd` only computes output dimensions;
  - should produce actual tile pixel content.
- [ ] **Add tests for mip generation correctness**:
  - dimensions for each level,
  - pixel parity against scalar reference on small fixtures,
  - lazy behavior: verify levels are generated on demand only.
- [ ] **Optional performance enhancement**:
  - AVX2/SIMD path for 2×2 box filter (keep scalar fallback).

### Suggested files to modify for Step 6 completion

- `pixors-engine/src/image/mip.rs`
- `pixors-engine/src/server/service/tab.rs`
- `pixors-engine/src/storage/tile_store.rs`
- `pixors-engine/src/storage/tile_cache.rs`
- (if needed) new module under `pixors-engine/src/image/` for mip generation kernels.

---

## 9.3 Detailed Handoff – Step 8 (MCP Bridge Update)

### Current status

- [ ] `pixors-mcp` is **not yet migrated** to the tab/event-bus protocol.
- [ ] No end-to-end stdio test for tab lifecycle + screenshot exists yet.

### Required work (for another model)

- [ ] **Migrate MCP tools to tab-based REST/WS contracts**:
  - `pixors_create_tab` → `POST /api/tabs`
  - `pixors_open_image` → `POST /api/tabs/:id/open`
  - `pixors_close_tab` → `DELETE /api/tabs/:id`
  - `pixors_activate_tab` → WS `ActivateTab`
  - `pixors_select_tool` → WS `SelectTool`
  - `pixors_get_state` → `GET /api/state`
  - `pixors_screenshot` → WS `Screenshot` (or temporary REST endpoint if needed)
- [ ] **Implement MCP-side event listener** for engine events:
  - keep local MCP state synchronized from events only,
  - do not mutate local state on command send.
- [ ] **Define screenshot transport contract**:
  - expected payload: base64 PNG + metadata (tab_id, width, height),
  - handle large payload safely over stdio.
- [ ] **Add robust connection management**:
  - reconnect WS,
  - backoff + clear errors,
  - no duplicate subscriptions after reconnect.
- [ ] **Add integration tests (stdio + running engine)**:
  - create → open image → get state → screenshot → close,
  - assert events observed in order,
  - assert MCP and UI stay in sync when MCP triggers commands.

### Suggested files/modules to touch in `pixors-mcp`

- MCP transport/client layer (HTTP + WS wiring)
- Tool handlers for tab lifecycle and screenshot
- MCP event dispatcher/state sync module
- Integration test harness (stdio scenario runner)

---

## 10. Dependencies

```toml
# pixors-engine/Cargo.toml
lru = "0.12"
tempfile = "3"
```

---

## 11. Success Criteria

- [~] Opening a 50 MP PNG loads only visible tiles to RAM
- [~] Closing a tab frees ALL resources (RAM, temp files, WS)
- [~] Pan/zoom smooth at 60 fps
- [ ] Pixel grid appears at high zoom
- [ ] MCP can create tabs, open images, switch tools, take screenshots
- [ ] MCP actions reflected in UI in real-time (event-driven)
- [x] UI actions round-trip through engine (no direct state mutation)
- [~] SIMD tile extraction ≥3× faster than scalar
- [~] `cargo test` passes, `cargo check` clean
