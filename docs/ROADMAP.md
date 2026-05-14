# Roadmap

This is a living document. Phases are revised as we learn more — items move,
split, merge, and get reprioritized as the project evolves. If something here
contradicts a recent decision, the recent decision wins.

Items are organized by phase where sequencing is known, and collected in a backlog
section for everything not yet scheduled. Nothing outside "In progress" is committed
or scoped — phases exist to communicate intent and dependencies, not deadlines.

---

## ✓ Complete — Phase 9

Phase 9 delivered the core engine architecture:
- `Action` trait + `Dispatcher` with per-tab pipeline locking
- `ActionChain` typed wrapper; `Dispatcher::run_graph()` for viewport-only pipelines
- `pixors-document` cleaned to headless model layer (no GUI deps, no viewport display code)
- Viewport display state (TileCache, Camera, ViewportState, TileCacheSink, TileCacheSource) moved to `pixors-desktop`
- GPU buffer race condition fixed: input `Arc<GpuBuffer>` retained in `EncoderSlot::keep_alive_gpu` until after `queue.submit()`; `pool.recycle_pending()` guarded by `device.poll(Wait)`
- Export modal UI (PNG + TIFF, full config)

---

## ✓ Complete — Phase 10

**Goal:** First complete editing loop — open → layer controls → per-layer filters → composite display → export.

Phase 10 delivered:
- Layer panel with visibility toggle, opacity slider (live preview on drag, commit on release), drag-to-reorder
- Filter panel with add/remove transforms, blur slider preview (live `mutation.apply(doc)` + `compile()` + `run_render()` pipeline), per-filter expand/collapse
- Preview = apply + compile + run — zero intermediate overlay cache. `pending_preview` mutation undone before commit
- `Operation::label()`, `subtitle()`, `color()` methods on Operation enum, filter panel uses them instead of hardcoded match arms
- Single `run_render(session_id, mip, range)` entry point, `compile_config()` helper dedup, unified `viewport_mip_range()`
- `PipelineHandle::Drop` + `Dispatcher::Drop` cancel+join threads (no zombie threads on shutdown)
- Loading overlay with animated canvas spinner, percentage, dark backdrop — session pushed immediately (no gap between dialog and feedback)
- SPV shader hash caching — incremental builds skip `slangc` (10s → 1.3s)
- Buffer pool debug logs removed (17 log lines), `blur_preview_radius` removed from `App`, `opacity_overrides` and `compile_preview` deleted

---

## Phase 11 — Format support + Blend modes + Library workspace v1 + Smart Render Cache

**Goal:** support every common still-image format, complete the separable blend-mode
set on both CPU and GPU compositors, ship a v1 Library workspace, and add a
disk-backed render cache that makes repeated applies, undo/redo, and Export fast.

See [PHASE_11.md](PHASE_11.md) for the full implementation plan.

- **Smart Render Cache** (prerequisite for everything else): per-session disk
  cache keyed by `(source fingerprint, transform-prefix params)` with a dual-pool
  layout — small slot count of mip-0 caches (large f16 entries) and a larger slot
  count of mip-N caches (small previews). Refactors `render/compiler.rs` into a
  `Compile` trait implemented by `LayerNode`, `Transform`, `Operation` (and future
  effects). RAM LRU auto-evicts on read, not only on write.
- **Undo/redo wired up** (`History` exists; controller/UI bindings ship here).
  Cache hits make every recent undo instant; deep undo past the slot cap
  recomputes.
- **JPEG hardening, WEBP (animated frames as pages), AVIF, EXR, multi-page TIFF**
  via a new `DecoderRegistry`. Each enters the existing tile pipeline as a new
  `ImageDecoder` impl.
- **Blend modes** — Normal (already ships in Phase 10), plus Multiply, Screen,
  Overlay, Soft Light, Hard Light, Color Dodge, Color Burn, Difference, Exclusion.
  CPU + GPU paths together. The blend-mode dropdown lives in the layers panel
  header (above the layer list). Luminosity / Color / Hue / Saturation deferred
  (need Lab compositor — revisit with Darkroom).
- **Thumbnails**: long-lived per-file PNG thumb cache at
  `~/.cache/pixors/thumbs/`, EXIF-embedded thumbnail short-circuit when present,
  plus 48×48 per-layer thumbnails inside the layers panel rows.
- **Library workspace v1**: file browser grid, open into Layer Editor, rating /
  pick / reject via Lightroom-compatible XMP sidecar, basic EXIF/IPTC summary
  panel. Detailed UI spec lives in a separate doc.
- EXIF/IPTC write into the file itself, smart collections, persistent-across-sessions
  cache, and GPU-resident cache are deferred (Phase 12 / Phase 14).

---

## Phase 12 — RAW v1 (Canon CR3 + baseline algorithms) + GPU-resident cache

**Goal:** RAW decode working end-to-end on Canon CR3. Algorithms first, camera
model breadth later. CR3 chosen because Canon hardware is available for testing.

- Demosaicing (AHD or equivalent quality)
- White balance from camera metadata
- Color matrix: sensor primaries → ACEScg
- Capture noise reduction (applied before any user op, not part of the op stack)
- Base tone curve from camera profile
- CR3 decode plugged into the existing `TileSource` interface — the rest of the
  pipeline is unchanged

**Plus** (carried over from Phase 11 deferred):

- **GPU-resident render cache** — extend the Phase 11 Smart Render Cache so hot
  tiles can stay on the GPU between dispatches, skipping the Download/Upload
  round-trip when both producer and consumer of a cache hit are GPU-assigned.
  Requires a per-tile lifetime/ref-count layer on `Arc<GpuBuffer>` so eviction
  cannot race with in-flight dispatches.
- **Cache stats panel** — surface hits, misses, RAM use, disk use per pool
  (Mip0 / MipN / GPU). Drives both user trust ("is my undo really cached?") and
  the perf-tuning loop we'll need while landing RAW.

---

## Phase 13 — RAW v2 (format breadth + profiles)

- NEF (Nikon), ARW (Sony), DNG (universal baseline)
- Camera color profiles (DCP or equivalent)
- Improved capture noise reduction
- HEIC / HEIF decode — depends on platform codec availability
- Additional Canon models beyond CR3 if needed

---

## Phase 14 — Darkroom workspace v1

**Goal:** a second workspace for non-destructive photo adjustment. No layers.
Every op is a parameter set applied in sequence; the result is cached as tiles
at each MIP level. Recompute only when a parameter changes.

- Workspace shell: separate from Layer Editor, own panel layout
- Non-destructive op pipeline: ordered list of ops with editable parameters,
  previewed MIP-aware (same system as Phase 9), applied on commit
- Tonal ops: Exposure (EV stops), Brightness, Contrast, Highlights, Shadows,
  Whites, Blacks, Tone Curve (RGB + per-channel), Levels
- Color ops: White Balance (temperature + tint), Hue / Saturation / Luminance
  (global), HSL per hue range (8 ranges), Vibrance, Color Grading
  (lift / gamma / gain wheels)
- Export from Darkroom routes to the Phase 10 export modal
- **Histogram panel** — real-time luminance and per-channel histogram computed
  from the current viewport MIP level via a new `viewport_histogram` engine
  command reading the display tile cache. Lives in the Darkroom side panel and
  drives the white/black-point pickers on Levels and Curves.
- **Persistent render cache across sessions** — extend the Phase 11 Smart Render
  Cache so cached tiles survive app restarts. Keyed by `(source fingerprint,
  transform-prefix params)` exactly as in-session, plus an additional age/size
  policy at the file-system level (`~/.cache/pixors/render/`). Darkroom is the
  natural home: non-destructive op stacks are stable across sessions, so the
  hit rate is high; the Library workspace also gets quick re-opens of recent
  edits for free.

**Architecture note:** all ops are non-destructive — a sequence of parameters is
cheap to store; the tile cache is the performance layer, not the source of truth.
Recompute from parameters when the user edits an earlier step; serve from cache
otherwise.

---

## Phase 15 — Masking engine

**Goal:** a masking system strong enough for both Darkroom and Layer Editor.
The toolset available in each workspace varies; the underlying engine is shared.

- SAM (current best small YOLO-family model) integrated as a sidecar process —
  runs offline, small enough for most hardware, communicates with the engine over
  IPC and returns mask data
- Precise matting for edge refinement (hair, fur, semi-transparency)
- Shared masking panel — appears in both workspaces, adapted to context:
  - Darkroom: subject selection → mask for localized adjustment
  - Layer Editor: subject selection → layer mask
- Geometric selection tools: rectangular marquee, elliptical marquee, freehand
  lasso, polygonal lasso
- Brush tools: paint mask, erase mask, refine edge brush
- Feather / expand / contract controls

---

## Phase 16 — Selection engine (Layer Editor)

- Quick selection (region grow by similarity)
- Magic wand (flood fill by tolerance)
- Color range selection
- Luminance mask (select by brightness range)
- Feather / expand / contract / inverse
- Quick mask mode (paint selection as red overlay)
- Transform selection (scale, rotate the marquee before committing)

---

## Phase 17 — Layer Editor: adjustments + geometry ops

- Sharpen / Unsharp mask (radius + amount + threshold) — in Adjustments panel
- Crop tool with aspect lock and composition overlay (rule of thirds, grid,
  golden ratio)
- Rotate arbitrary angle with canvas expansion
- Flip horizontal / Flip vertical
- Vignette (radial, color, feather)
- Grain / film grain (luminance-weighted noise)
- Color grading wheels (same as Darkroom, applied as a layer op)

---

## Unphased backlog

Everything below is captured intent. Order and grouping will be decided when the
preceding phases are stable.

---

### Per-stage cooperative cancellation

Pipeline-level cancel already exists: `Pipeline::compile` carries an
`Arc<AtomicBool>`, `PipelineHandle::cancel` flips it, and `PipelineHandle::Drop`
joins all chain threads. What is *not* there: each `Producer`/`Processor` checking
the flag mid-work. A long-running blur or color convert keeps running until it
returns from its current call into the runtime. Adding a `ctx.cancelled()` check
inside hot loops would make tab-close and job-cancel feel instant on large images.

**Depends on:** Dispatcher/pipeline system (Phase 9, complete). Low risk; do
when an op visibly blocks tab close on a real image.

---

### Lock zoom during active preview

While a Preview job is running, block zoom gestures on the frontend and reject
`ViewportCommand::Update` on the backend. The preview is bound to a fixed MIP
level; zoom mid-preview discards visible tiles and triggers a new preview at the
new level — cancel-on-zoom works but is flickery.

**Depends on:** Preview system (Phase 10). Deferred — complicates tile scheduling.

---

### Unified display + storage MIP pipeline

Two separate MIP pyramids exist: the display tile cache (sRGB u8, RAM, written
by `TileCacheSink` in `pixors-desktop`) and the storage tile cache (ACEScg f16,
disk, written by `CacheWriter` via the `MipDownsample` stage in
`pixors-ops`). They serve different purposes and do not currently interact —
intentional. If duplication becomes a problem, revisit as an optimization, not
a correctness fix.

---

### Layer effects

Drop shadow, inner shadow, outer glow, inner glow, bevel/emboss, color overlay,
gradient overlay, pattern overlay, stroke. Each is a post-composite op applied to
the layer's bounding box.

**Depends on:** blend modes (Phase 11), layer groups.

---

### Layer groups and clipping masks

Group layers into a folder with a shared blend mode and opacity. Clipping mask:
a layer's pixels are clipped to the alpha of the layer directly below.

**Depends on:** blend modes (Phase 11).

---

### Content-aware fill / inpainting

Replace a selected region with synthesized content that matches the surroundings.
Requires a generative model or classical patch-based synthesis (PatchMatch).
Large scope — may warrant its own phase.

**Depends on:** selection engine (Phase 16), masking (Phase 15).

---

### Retouching tools

Clone stamp, healing brush, spot heal, patch tool. All require painting into the
ACEScg tile store at MIP-0 and invalidating higher MIPs.

**Depends on:** selection engine (Phase 16).

---

### Perspective correction and lens distortion

Keystone correction (four-corner warp), barrel/pincushion distortion removal,
chromatic aberration correction. Lens profiles from Lensfun or Adobe LCP.

**Depends on:** geometry ops (Phase 17).

---

### Content-aware scale (seam carving)

Resize the canvas by removing or duplicating least-important seams. Protect
regions via a painted mask.

**Depends on:** masking (Phase 15), geometry ops (Phase 17).

---

### Noise reduction (user-facing)

Luminance noise reduction and color (chrominance) noise reduction as user-applied
ops in the Darkroom and Layer Editor. Distinct from capture noise reduction in the
RAW pipeline (Phase 12), which runs before any user op and is not user-configurable.

**Depends on:** Darkroom ops (Phase 14).

---

### Dehaze

Atmospheric scattering removal. Dark-channel prior or learned model. Fits in the
Darkroom op stack alongside the tonal ops.

**Depends on:** Darkroom ops (Phase 14).

---

### Clarity / Texture / Local contrast

Mid-frequency local contrast enhancement. Unsharp mask applied at a large radius
with a low amount, or guided filter equivalent. Useful in both Darkroom and Layer
Editor.

**Depends on:** Darkroom ops (Phase 14).

---

### Properties panel

Pixel dimensions and offset for the active layer, with editable fields that
dispatch new `SetLayerOffset` / `ResizeLayer` mutations (analogous to the
existing `SetLayerOpacity` / `SetLayerBlend` in `pixors-document::mutation::impls`).
The data already exists on `LayerNode` and `CanvasInfo` — this panel just needs
the mutations + a small UI surface.

**Depends on:** Phase 10 layer wiring (complete). Low effort, deferred alongside
Histogram.

---

### 3D LUT / HALD CLUT import

Apply an external color lookup table (`.cube`, HALD PNG) as an op in Darkroom or
Layer Editor. The LUT infrastructure already exists in `color/` — this is mostly
a file loader and a UI entry in the op list.

**Depends on:** Darkroom ops (Phase 14).

---

### HDR tone mapping

Reinhard, ACES filmic, Hable — applied to f32 images. The pipeline already
supports f32 via EXR decode. Relevant once there is a reason to open HDR content
(EXR decode in Phase 11, Darkroom in Phase 14).

**Depends on:** EXR decode (Phase 11), Darkroom ops (Phase 14).

---

### Multi-exposure merge (HDR, focus stack, panorama)

Align and merge bracketed exposures into a single f32 image. Panorama stitching
via homography + blending. Focus stacking via depth-from-defocus. Large scope —
deserves its own phase when the time comes.

---

### Smart collections (Library)

Virtual albums defined by rules: ISO > 1600, rating ≥ 4, lens = 50mm, date range,
etc. Evaluated lazily against the file database.

**Depends on:** Library workspace (Phase 11).

---

### Face detection and grouping (Library)

Detect and cluster faces across the library. Tag faces with names, filter by
person. Requires an embedded face model (small, offline).

**Depends on:** Library workspace (Phase 11).

---

### EXIF / IPTC write + XMP sidecar

Write rating, flag, keywords, and caption back to the file or a `.xmp` sidecar.
Needed for round-tripping with other tools (Lightroom, digiKam).

**Depends on:** Library workspace (Phase 11).

---

### Text tool

Raster-only: render text to a new layer, bake on commit. No live editing after
bake. Basic font picker, size, color, alignment.

---

### Basic shapes

Rectangle, ellipse, line — rasterized immediately to a new layer. No vector layer.
Useful for annotation without adding a vector engine.

---

### MCP tool surface

Grows alongside the product. Since the Phase 9 split, `pixors-document` is
already headless and reachable from `pixors-mcp` without a window. Near-term
priorities: `query_pixels` (return a region as base64 PNG or raw f16),
`apply_mutation`, `export`. The MCP server crate lives in `pixors-mcp/`. See
`docs/MCP_INTEGRATION.md`.

---

### Distribution and code-signing

GH Actions already packages the binary per platform. Code-signing (notarization
on macOS, Authenticode on Windows) and store distribution (Mac App Store, Flatpak)
are future concerns — not blocking any feature work.

---

## Not on the roadmap

These are explicitly **not** planned, recorded here so they don't keep coming
back as ad-hoc requests:

- **Animated image authoring or playback** — animated WEBP / APNG / GIF are
  decoded as a stack of layers (Phase 11) and that's the entire affordance.
  There is no timeline UI, no frame-stepping, no animation export, and no
  intent to add one. Pixors is a still-image editor; animation is a different
  product.
- **Video** — same reasoning. Out of scope at the product level.