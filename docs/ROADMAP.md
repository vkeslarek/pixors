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
- `pixors-state` cleaned to headless model layer (no GUI deps, no viewport display code)
- Viewport display state (TileCache, Camera, ViewportState, TileCacheSink, TileCacheSource) moved to `pixors-desktop`
- GPU buffer race condition fixed: input `Arc<GpuBuffer>` retained in `EncoderSlot::keep_alive_gpu` until after `queue.submit()`; `pool.recycle_pending()` guarded by `device.poll(Wait)`
- Export modal UI (PNG + TIFF, full config)

---

## In progress — Phase 10

**Goal:** First complete editing loop — open → layer controls → per-layer filters → composite display → export.

See `docs/PHASE_10.md` for the full refined spec.

---

## Phase 11 — Format support + Blend modes + Library workspace v1

**Goal:** support every common image format, deliver blend modes for the compositor,
and deliver the Library workspace. Multi-layer TIFF files land in this phase — they
are immediately usable only if blend modes are also present, so the two ship together.

- Decode JPEG (moved from Phase 10 if not shipped there), WEBP, AVIF, EXR — each
  enters the existing tile pipeline as a new `ImageDecoder` impl; the rest of the
  pipeline is unchanged.
- EXR decode: f16/f32 enters the pipeline naturally; no special casing needed.
- Multi-layer TIFF: expose each TIFF page as a separate `Layer` in the tab, each
  with its own cache subdirectory (layout already defined in Phase 10).
- **Blend modes** — Normal (already ships in Phase 10), plus:
  Multiply, Screen, Overlay, Soft Light, Hard Light, Color Dodge, Color Burn,
  Difference, Exclusion. Luminosity, Color, Hue, Saturation deferred (need Lab
  conversion in the compositor — revisit when Darkroom color science lands).
  Implemented in `Compose` CPU path first; GPU path is a follow-up optimization.
  Unblocks: layer groups, clipping masks, layer effects (all currently backlog).
- Thumbnail extraction and disk cache (generated from MIP-N during decode,
  reused in Library grid).
- Library workspace v1:
  - File browser grid with thumbnails
  - Open file into Layer Editor
  - Rating and flag (pick / reject) — stored in sidecar or embedded XMP
  - Basic EXIF / IPTC read for metadata display
- EXIF/IPTC write and smart collections deferred to a later Library pass.

---

## Phase 12 — RAW v1 (Canon CR3 + baseline algorithms)

**Goal:** RAW decode working end-to-end on Canon CR3. Algorithms first, camera
model breadth later. CR3 chosen because Canon hardware is available for testing.

- Demosaicing (AHD or equivalent quality)
- White balance from camera metadata
- Color matrix: sensor primaries → ACEScg
- Capture noise reduction (applied before any user op, not part of the op stack)
- Base tone curve from camera profile
- CR3 decode plugged into the existing `TileSource` interface — the rest of the
  pipeline is unchanged

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

### Undo / redo

Tile-level history: store pre-op tile snapshots in a slab allocator with a
configurable memory cap. When cap is hit, flush oldest entries to a temporary
directory. Re-apply from the snapshot rather than re-running the op chain.
Non-destructive ops in Darkroom make this simpler — history is just the parameter
sequence, no tile snapshots needed for that workspace.

**Depends on:** Phase 10 operations, architecture decision in Phase 14.

---

### Cancellation tokens

Pipes do not currently listen to cancellation signals. If `close_image` fires
mid-load, threads run until `StreamDone`. A `CancellationToken` checked at the
top of each pipe loop would make tab close and job cancel instant.

**Depends on:** Dispatcher/pipeline system (Phase 9, complete). Revisit after job architecture is stable.

---

### Lock zoom during active preview

While a Preview job is running, block zoom gestures on the frontend and reject
`ViewportCommand::Update` on the backend. The preview is bound to a fixed MIP
level; zoom mid-preview discards visible tiles and triggers a new preview at the
new level — cancel-on-zoom works but is flickery.

**Depends on:** Preview system (Phase 10). Deferred — complicates tile scheduling.

---

### Unified display + storage MIP pipeline

Two separate MIP pyramids exist: display (sRGB u8, RAM, via `MipPipe`) and storage
(ACEScg f16, disk, via `generate_from_mip0`). They serve different purposes and
do not currently interact — this is intentional. If duplication becomes a problem,
revisit as an optimization, not a correctness fix.

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

### Histogram panel

Real-time luminance and per-channel histogram computed from the current viewport
MIP level. Needs a `viewport_histogram` engine command that reads from the Viewport
RAM cache and returns 256-bucket per-channel data.

**Depends on:** Phase 10 composite display. Deferred until core image processing
is stable — no point displaying accurate histograms before ops are complete.

---

### Properties panel

Pixel dimensions and offset for the active layer, with editable fields that
dispatch `Layer.SetOffset` and eventually `Layer.Resize`. The data already exists
in `ImageLoaded` and `LayerEvent::Changed` — this panel just needs wiring.

**Depends on:** Phase 10 layer wiring. Low effort, deferred alongside Histogram.

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

Grows alongside the product. Every `EngineCommand` exposed over WebSocket is
automatically reachable from MCP clients via `pixors-server`. Near-term priorities
once Phase 10 ships: `query_pixels` (return a region as base64 PNG or raw f16),
`apply_operation`, `export`. The MCP server crate lives outside this repository.
See `docs/MCP_INTEGRATION.md`.

---

### Distribution and code-signing

GH Actions already packages the binary per platform. Code-signing (notarization
on macOS, Authenticode on Windows) and store distribution (Mac App Store, Flatpak)
are future concerns — not blocking any feature work.