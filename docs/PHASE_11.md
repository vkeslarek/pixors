# Phase 11 — Format Support + Blend Modes + Library workspace v1 + Smart Render Cache

> Status: planning. Phase 10 is complete (3803cef). This document is the full implementation plan
> for Phase 11 plus a cross-cutting "Smart Render Cache" refactor that the roadmap promises and
> the rest of Phase 11 depends on.
>
> Audience: an AI engineer (or human) implementing the phase end-to-end. Every section below is
> intended to be unambiguous: it tells you *which files to touch*, *what shape the new code
> takes*, *what invariants must hold*, and *why the design is the way it is*. When two designs
> were considered, the one not chosen is recorded with the reason.

---

## 0. Table of contents

1. Phase 11 goals & scope
2. Architectural overview — what changes vs Phase 10
3. **Workstream A — Smart Render Cache** (prerequisite for the rest)
4. **Workstream B — Compositor blend modes**
5. **Workstream C — Decoder breadth** (JPEG hardening, WEBP, AVIF, EXR, multi-page TIFF, multi-frame WEBP)
6. **Workstream D — Thumbnails + thumbnail cache**
7. **Workstream E — Library workspace v1**
8. Cross-cutting tasks (CLAUDE.md, ARCHITECTURE.md, tests)
9. Acceptance criteria & order of merge
10. Out of scope / explicitly deferred

---

## 1. Phase 11 goals & scope

From `docs/ROADMAP.md` (Phase 11), three deliverables:

- **Format breadth**: every common still-image format readable. JPEG, WEBP (incl. animated),
  AVIF, EXR. Multi-page TIFF surfaces each page as a layer. JPEG/WEBP already have stub
  decoders (`pixors-image/src/jpeg/mod.rs`, `pixors-image/src/webp/mod.rs`); they currently
  decode the whole file into RAM up-front and emit it scanline-by-scanline through
  `PageStream::drain`. That works for small JPEGs and breaks for 100MP files. We harden them
  and add the remaining decoders.
- **Blend modes**: Normal already ships. Add Multiply, Screen, Overlay, Soft Light, Hard
  Light, Color Dodge, Color Burn, Difference, Exclusion — both CPU (in `Compose::cpu_compose`)
  and GPU (in `pixors-shader/shaders/compose.slang` plus `Compose::gpu_compose`).
  Luminosity / Color / Hue / Saturation are explicitly deferred (need a Lab-conversion compositor
  variant — revisit when Darkroom color science lands in Phase 14).
- **Library workspace v1**: a second workspace alongside the Layer Editor, with a file
  browser grid, thumbnails, open-into-editor, rating + flag, and basic EXIF/IPTC display.

Plus the user-requested cross-cutting requirement:

- **Smart Render Cache**: a disk-backed cache of intermediate tile buffers, keyed by
  layer + transform-stack prefix. Used to make repeated edits, undo/redo, and export fast by
  replaying only the tail of the pipeline that actually changed. Includes a refactor of
  `pixors-document/src/render/compiler.rs` so the cache hook-up is clean.

The Smart Render Cache is sequenced **first** (Workstream A) because:

- Blend modes will produce a much larger combinatorial space of composite results; users will
  flip between them and we don't want to re-run blur every time.
- Library workspace thumbnails are themselves a render-cache use case (the cached composed
  display-space tile at mip=N *is* the thumbnail).
- Export benefits immediately (composite cache reused at mip=0).
- It touches the same files (`render/compiler.rs`, `Compose`) that the blend-mode work touches,
  so doing it after would force a rebase.

---

## 2. Architectural overview — what changes vs Phase 10

Today (Phase 10):

```
visible render:
  for each visible layer:
      CacheReader(layer_cache, mip)
        → TileToNeighborhood → Blur            \
        → … (all transforms re-run every call)  → Compose → ColorConvert → TileCacheSink
                                                /
```

Every viewport refresh, every slider tick, every undo, recompiles the same graph and re-runs
every operation. There is *one* disk cache per layer and it only holds the post-decode pixels
at each mip — nothing downstream is cached.

After Phase 11:

```
visible render:
  for each visible layer:
      // resolve transform prefix via RenderCache lookups
      CacheReader(layer_cache, mip)
        → [Blur] → CacheWriter(key=H(layer, [blur_params]))      ← only if miss
        → [Levels] → CacheWriter(key=H(layer, [blur, levels]))   ← only if miss
                                                                  \
        → Compose → CacheWriter(key=H(visible layer-keys, blendspecs))
                  → ColorConvert → CacheWriter(display key)
                                 → TileCacheSink

  On hit at any cache point, the producer becomes `CacheReader(that key)` and *all earlier
  stages for that layer are dropped from the graph*.
```

Concrete changes by crate:

| Crate | Phase-11 changes |
|---|---|
| `pixors-engine`  | `cache::render_cache` (new): keyed multi-cache + LRU disk eviction. Blend-mode enum gains 9 variants. |
| `pixors-shader`  | `compose.slang`: branchless `apply_blend_mode()` helper, dispatched per layer. |
| `pixors-image`   | JPEG: stream-decode via `zune_jpeg::JpegDecoder::decode_into` per-block; or, easier, keep one-shot decode but stream rows lazily — see §C.1. WEBP animated, AVIF (libavif), EXR (exr crate). Decoder registry. |
| `pixors-ops`     | `Compose`: extend CPU + GPU paths to 10 blend modes. |
| `pixors-document`| `render/compiler.rs` rewrite around a `CompilePlan` abstraction. New `document::workspace` enum. `document::rating` field on assets. `mutation::impls`: new `SetRating`, `SetFlag`. Thumbnail extraction action. |
| `pixors-desktop` | Library workspace page; workspace switcher in the menubar; new thumbnail grid widget. Existing Layer Editor untouched except the controller calls the new compile path. |

---

## 3. Workstream A — Smart Render Cache

### A.1 Why a new cache and not extend `DiskCache`

`pixors-engine::cache::disk_cache::DiskCache` is *per-layer-source* — its directory is
`session/layer_{id}/mip_X/tile_X_Y_Z.raw` and its key is `(mip, tx, ty)`. We need a cache whose
key is *the entire transform prefix*. Two options were considered:

| Option | Decision | Reason |
|---|---|---|
| Extend `DiskCache` with an extra key parameter | reject | Tile path becomes `mip/tile_TX_TY/key_HEX.raw` per tile — fan-out of millions of tiny files in one dir, slow on every OS. Also forces `CacheReader`/`CacheWriter` to learn about keys, polluting their API. |
| New abstraction: `RenderCache` that owns *many* `DiskCache`s, one per key | **accept** | Each key gets its own subdirectory, reusing the existing `DiskCache` layout unchanged. `CacheReader` and `CacheWriter` are reused verbatim. LRU eviction at the *cache* granularity, which is the natural unit: when a transform's params change, the whole keyed cache becomes stale and is deleted as one unit. |

### A.1.1 Split disk budgets: mip-0 vs mip-N caches

Working storage is f16 RGBA. 24MP at mip-0 = ~192 MiB *per cache entry*. We cannot let the
render cache treat mip-0 entries as equivalent to thumbnail-sized mip-5 entries — the budget
collapses to ~10 entries on disk for a moderately-sized session.

Decision: **two independent disk pools** managed by the same `RenderCache`.

| Pool | Holds | Default budget | Slot count cap |
|---|---|---|---|
| **`Mip0Pool`** (full-resolution) | only entries at `mip == 0` | 4 GiB | 8 slots (PIXORS_MIP0_CACHE_SLOTS) |
| **`MipNPool`** (preview/zoomed) | entries at `mip >= 1` | 1 GiB | 64 slots (PIXORS_MIPN_CACHE_SLOTS) |

Rationale:

- User mostly interacts at fit-to-screen (some mip N ≥ 1). MipN pool gets the high hit rate
  and small per-entry size, so it can hold many recent param states (slider history) cheaply.
- Mip-0 is touched only on Export, 1:1 zoom, or explicit pixel-peeping. A small slot count is
  fine — once you've moved past a state at mip-0, recomputing is acceptable.
- A deeply-stacked undo history naturally trades disk for compute: once the user undoes past
  the slot count, the older states fall out and rebuild on next visit. That's the trade-off
  the user wants and the doc states it explicitly: **deep undo recomputes, recent undo is
  instant**.

Both pools are LRU. Eviction is per-pool: filling Mip0 never evicts MipN and vice-versa.

API shape:

```rust
pub struct RenderCache {
    root: PathBuf,
    mip0: Mutex<KeyedPool>,
    mipn: Mutex<KeyedPool>,
}

struct KeyedPool {
    entries: indexmap::IndexMap<RenderKey, Arc<DiskCache>>,  // insertion order = LRU order
    bytes: u64,
    budget_bytes: u64,
    slot_cap: usize,
}

impl RenderCache {
    pub fn get_or_create(&self, key: RenderKey, mip: u32) -> Arc<DiskCache> {
        let pool = if mip == 0 { &self.mip0 } else { &self.mipn };
        // touch (move to back), evict head until under budget and slot cap, insert
    }
    pub fn try_hit(&self, key: RenderKey, mip: u32, range: &TileRange) -> Option<Arc<DiskCache>>;
}
```

`indexmap::IndexMap` gives O(1) LRU with `shift_remove` + `insert` at tail. Use it (already a
common dep in the workspace; if not present add it — `indexmap` is permissively licensed and
~30 KiB compiled).

### A.2 Data model

New file: `pixors-engine/src/cache/render_cache.rs`.

```rust
/// Stable hash identifying a cached render result.
///
/// Two graphs that produce byte-identical tiles for the same (mip, tx, ty) MUST have the
/// same RenderKey. Two graphs that don't, MUST NOT. The hash inputs below uphold both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderKey(pub [u8; 16]);  // first 128 bits of BLAKE3

impl RenderKey {
    pub fn hex(&self) -> String { /* lowercase 32-char */ }
}

/// Inputs that go into a RenderKey via BLAKE3:
///   "pixors.render_cache.v1\0"             // version tag — bump to invalidate everything
///   working_format        (u32 le)
///   working_color_space   (u32 le serialization — must be stable; see A.2.1)
///   tile_size             (u32 le)
///   img_w, img_h          (u32 le each)
///   source_layer_id       (u64 le)         // disambiguate multi-layer source data
///   source_asset_fingerprint (32 bytes)    // see A.2.2
///   for each transform in the prefix, in order:
///       transform.enabled (u8)
///       discriminant(transform.op)       (u8)
///       Operation::params_hash()         (u64 le)        // already exists
///       discriminant(transform.input)    (u8)
///       if Reference(n): n               (u64 le)
///       discriminant(transform.output)   (u8)
///       blend_spec_of_output             (BlendMode u8 + opacity_bits u32)
```

#### A.2.1 `ColorSpace` and `PixelFormat` serialization for hashing

Both are `Copy` enums but their `Hash` impls today rely on derived `Hash` which uses the
*pointer-stable* discriminant. That's fine within one process run but *not* across runs (Rust
gives no stability guarantee). For disk-keyed caches we need cross-run stability. Add to
`pixors-engine/src/common/pixel/format.rs` and `…/color/space.rs`:

```rust
impl PixelFormat {
    /// Stable integer identifier for on-disk hashing. Adding a new variant must give it
    /// a new explicit number and never reuse a retired one.
    pub fn stable_id(self) -> u32 { /* match every variant explicitly */ }
}
impl ColorSpace { pub fn stable_id(self) -> u32 { /* ditto */ } }
```

Property test: round-trip every variant; CI fails if any matches `0` or duplicates.

#### A.2.2 Source asset fingerprint

When the user re-opens the same file, we want cache hits to survive. Fingerprint =
`BLAKE3(path bytes ++ file mtime as u64 ++ file size as u64)`. *Don't* hash file contents — too
slow on multi-GB raws. The mtime+size fingerprint is the same trick rsync uses; collisions
require the user to have *replaced* a file with one of the same size and exact mtime, which
both never happens accidentally and produces no visual artefact worse than the wrong cache
(cleared via "Reset cache" menu).

Stored in `pixors-document::document::asset::AssetStore` as `Option<[u8; 32]>` next to
`primary_path`. Populated in `ingest::prepare_ingest()` right after `open_image()`.

### A.3 `RenderCache` struct

```rust
pub struct RenderCache {
    root: PathBuf,                          // session_cache_dir/render
    mip0: Mutex<KeyedPool>,                 // see A.1.1
    mipn: Mutex<KeyedPool>,
    per_cache_memory: usize,                // forwarded to each DiskCache RAM LRU
}

impl RenderCache {
    pub fn new(root: PathBuf, mip0_budget: u64, mipn_budget: u64,
               mip0_slots: usize, mipn_slots: usize,
               per_cache_memory: usize) -> Self;

    pub fn get_or_create(&self, key: RenderKey, mip: u32) -> Arc<DiskCache>;
    pub fn try_hit(&self, key: RenderKey, mip: u32, range: &TileRange) -> Option<Arc<DiskCache>>;
    pub fn reset(&self);                    // wipe both pools
}
```

Implementation notes:

- `IndexMap` ordering = LRU order. `get_or_create` moves the hit entry to the tail; eviction
  pops from the head until both `bytes <= budget_bytes` *and* `len <= slot_cap`.
- Disk usage is computed lazily: each `DiskCache` exposes a `disk_size()` getter (new — sums
  `fs::metadata(path).len()` once at close-time, cached). The pool stores last-known sizes and
  rolls them into `bytes` on insertion / eviction.
- Eviction *deletes* the keyed cache (`DiskCache::cleanup()`) and removes the map entry. The
  `Arc<DiskCache>` outlives eviction if another thread is still reading it (Arc semantics) —
  but the underlying dir is gone, so subsequent reads return `None`. Document this as
  "in-flight reads may return partial misses near eviction." Acceptable.
- Session shutdown: `RenderCache` Drop = nuke the whole `render/` dir.

### A.3.1 RAM LRU auto-eviction inside `DiskCache`

Each `DiskCache` carries its own in-memory LRU (currently `max_memory` arg in
`pixors-engine/src/cache/disk_cache.rs::new`). Today eviction is **only triggered on write**
(`evict_if_needed` runs inside `write_tile`). A read-heavy workload (viewport pans → cache
hits → reads grow the LRU) never evicts because `read_tile` only inserts and never checks the
budget.

Fix in `disk_cache.rs`:

```rust
pub fn read_tile(&self, mip: u32, tx: u32, ty: u32) -> Option<Vec<u8>> {
    // … existing LRU hit path …
    // disk fallback:
    let data = fs::read(&path).ok()?;
    let size = data.len();
    let mut state = self.lru.lock().unwrap();
    state.evict_if_needed(size, self.max_memory);   // <-- add this call (currently absent on read)
    state.entries.insert(/* … */);
    state.mem_used += size;
    Some(data)
}
```

Plus convert the current "scan-min-by_key" eviction (O(n) per evict, linear in LRU size) to
`IndexMap` ordering so eviction is O(1): the *first* entry in insertion order is the LRU
victim, every `read_tile` / `write_tile` does `shift_remove` + `insert` to bump to tail.
Without this, large sessions degrade quadratically.

Also: a background "idle trim" — a single timer in `App::subscription` ticks once per 30 s
and calls `cache.trim_to(self.max_memory * 3 / 4)` on every live `DiskCache`, so an idle tab
shrinks its RAM footprint without waiting for the next read. Helps long sessions where the
user opens many files then narrows to one.

### A.4 Owning the `RenderCache`

`Session::transient` already owns `disk_caches: HashMap<NodeId, Arc<DiskCache>>` for layer
sources. Add a sibling field:

```rust
// pixors-document/src/session.rs
pub struct Transient {
    // … existing fields …
    pub render_cache: Arc<RenderCache>,
}
```

Initialised in `Transient::new(cache_dir)`:

```rust
let render_root = cache_dir.join("render");
let render_cache = Arc::new(RenderCache::new(
    render_root,
    /* budget */ 2 * 1024 * 1024 * 1024,           // 2 GiB default; setting later
    /* per-cache RAM LRU */ 16 * 1024 * 1024,      // 16 MiB
));
```

`CompileConfig` (in `render/compiler.rs`) gains:

```rust
pub struct CompileConfig {
    // … existing …
    pub render_cache: Arc<RenderCache>,
    pub source_fingerprint: [u8; 32],   // duplicated from AssetStore for hashing convenience
}
```

`controller/viewport.rs::compile_config` populates both fields.

### A.5 Refactoring `render/compiler.rs` — `Compile` trait

The current file is procedural: each function (`compile_layer`, `compile_transform`,
`compile_operation`) calls `graph.add_stage` directly and threads `CompileCtx` through. As we
add cache-aware compilation *and* anticipate future editor objects (special effects, masks,
adjustment layers, library-side rendering), the procedural form becomes a long match-arm pile.

Decision (replacing the original `LayerCompiler` design): introduce a generic `Compile` trait.
Each document-side object that contributes nodes to a render graph owns its own `impl Compile`.
The compiler becomes a *driver* that walks the document and calls `compile()` on each piece.

```rust
// pixors-document/src/render/compile_trait.rs (new)

/// Anything that contributes one or more Stages to the render graph.
/// Implementors live next to their data model:
///   - LayerNode  → impl Compile in document/layer.rs
///   - Transform  → impl Compile in document/transform.rs
///   - Operation  → impl Compile in document/transform.rs (called by Transform)
///   - Future: Effect, Group, Mask — each in its own file
pub trait Compile {
    /// Append this object's stages to the graph and return the StageId emitting its output.
    ///
    /// Receives the *upstream* output StageId (the input bus). May freely tee, fork, or
    /// short-circuit it via the cache, but MUST end with one well-defined output stage.
    fn compile(&self, cx: &mut CompileCtx<'_>, upstream: Option<StageId>) -> StageId;
}
```

Concrete impls:

```rust
// document/layer.rs
impl Compile for LayerNode {
    fn compile(&self, cx: &mut CompileCtx<'_>, _up: Option<StageId>) -> StageId {
        // 1. open layer source key (advance cx.current_key with source fingerprint + layer id)
        // 2. emit_source(): try cx.render_cache.try_hit(current_key); else CacheReader on layer DiskCache
        // 3. for each transform: t.compile(cx, Some(current_output)); tee a CacheWriter after each
        // 4. return current_output
    }
}

// document/transform.rs
impl Compile for Transform {
    fn compile(&self, cx: &mut CompileCtx<'_>, up: Option<StageId>) -> StageId {
        if !self.enabled { return up.expect("transform needs upstream"); }
        cx.advance_key(self);                                   // mix self into cx.current_key
        if let Some(hit) = cx.try_short_circuit() {
            return hit;                                         // cache substitution
        }
        let input = self.resolve_input(cx, up);                 // Layer/Below/Reference
        let out = self.op.compile(cx, Some(input));             // delegate to Operation
        cx.tee_cache_writer(out);                               // automatic tee on hit-miss
        out
    }
}

impl Compile for Operation {
    fn compile(&self, cx: &mut CompileCtx<'_>, up: Option<StageId>) -> StageId {
        let input = up.expect("operation needs upstream");
        match self {
            Operation::Blur { radius }    => emit_blur_chain(cx, input, *radius),
            Operation::Exposure { stops } => emit_exposure_stage(cx, input, *stops),
        }
    }
}
```

`CompileCtx` (was already in `compiler.rs`) is extended with the cache-walking state and a
small API for impls to call:

```rust
pub struct CompileCtx<'a> {
    pub doc: &'a Document,
    pub req: &'a RenderRequest,
    pub config: &'a CompileConfig,
    pub graph: ExecGraph,
    /// Running RenderKey for the *current branch* the walker is on.
    pub current_key: RenderKey,
    /// Stages this layer added — used to prune on cache short-circuit.
    pub current_layer_stages: Vec<StageId>,
}

impl CompileCtx<'_> {
    pub fn advance_key(&mut self, t: &Transform);
    pub fn try_short_circuit(&mut self) -> Option<StageId>;       // see §A.5.1
    pub fn tee_cache_writer(&mut self, src: StageId);
    pub fn record_stage(&mut self, id: StageId) -> StageId;       // tracks for pruning
}
```

The top-level driver is now tiny:

```rust
pub fn compile(doc: &Document, req: &RenderRequest, config: &CompileConfig, sink: Stage) -> ExecGraph {
    let mut cx = CompileCtx::new(doc, req, config);
    let layer_outputs: Vec<StageId> = doc.layers.iter()
        .filter(|l| l.visible && cx.layer_cache_dir(l.id).exists())
        .map(|l| { cx.reset_layer_scope(l); l.compile(&mut cx, None) })
        .collect();
    let composed = emit_compose(&mut cx, &layer_outputs);         // composite cache point inside
    let display  = emit_display_convert(&mut cx, composed);
    cx.attach_sink(display, sink);
    cx.finish()
}
```

#### Extension model

Adding a new editor concept later (e.g. an `Effect` like Drop Shadow):

1. Create the struct in `document/effect.rs` with serde + a `params_hash()`.
2. `impl Compile for Effect { … }` in the same file.
3. Add it to whatever owns it (likely a `LayerNode::effects: Vec<Effect>` for layer effects)
   and call `eff.compile(cx, Some(current))` inside `LayerNode::compile`.

No change to the compiler driver. No central match arm grows. The cache machinery applies
uniformly because `advance_key` is called before each child compile.

#### A.5.1 The pruning method `drop_disconnected_predecessors_of`

Today `ExecGraph` only adds nodes and edges; it has no removal helpers
(`remove_edge` exists, used by `insert_transfers`, but it does not GC unreachable nodes). For
the cache short-circuit we need to remove all stages added for *this layer so far* and replace
them with a single `CacheReader`. Two implementations:

| Approach | Decision |
|---|---|
| Add `ExecGraph::prune_subgraph_from(stage_id)` | reject — `ExecGraph` is the engine's lowest-level type; layering wrong. Also a pure topological prune may catch nodes that other layers share. |
| `LayerCompiler` records every `StageId` it added itself; on cache hit, removes them via existing `ExecGraph::remove_edge` + a new `remove_stage(id)` that is `unreachable!()` if the stage still has edges | **accept** — local, reasoned within the layer, doesn't need cross-layer analysis. |

Add `ExecGraph::remove_stage(id: StageId)` that panics if any edges remain on the node;
`LayerCompiler` is responsible for removing edges first.

#### A.5.2 Why tee instead of write-then-read

The naive design "after computing a transform, write to disk, *then* read it back to feed the
next stage" doubles I/O and serialises CPU work on disk. A tee — the producer feeds *both*
the next transform and a `CacheWriter` consumer in parallel — keeps the hot path entirely in
RAM/GPU and pays for the cache only with the extra disk write. `ChainRunner` already supports
fan-out: `build_channels` creates one `sync_channel` per `(src_chain, dst_chain, port)` tuple
and the runtime broadcasts via the `Emitter`.

#### A.5.3 Concurrency on writes

Two pipelines running in parallel (e.g. previous render still draining when the user moved a
slider and we kicked off a new one with the same RenderKey for the first transform) can both
try to write the same tile to disk. `DiskCache::write_tile` does `fs::write` which is atomic
for small files on POSIX (replaces in one call) but not on Windows. Two writes producing
identical bytes are fine; two writes interleaving are not. Mitigation: write to
`tile.raw.tmp.{pid}-{thread}` then `fs::rename`. Add this to `DiskCache::write_tile`.

### A.6 Cache use in the Compose stage

After per-layer transform stacks, layers feed `Compose`. We add a second cache point: a
*composite* key over **all visible layers** and their blend specs:

```
composite_key = BLAKE3(
    "pixors.compose.v1\0"
    for each visible layer (in stack order):
        layer.transform_prefix_key  (16 bytes)
        layer.blend.mode            (u8)
        layer.blend.opacity_bits    (u32 le)
)
```

The compiler:

1. Builds each layer's `LayerCompiler::run()` and gets back the layer-output `StageId` + its
   final `current_key`.
2. Computes `composite_key`.
3. Calls `render_cache.try_hit(composite_key, mip, range)`. On hit, drops every layer's
   subgraph and emits a single `CacheReader(composite_key)`.
4. On miss, runs `Compose` as today and tees a `CacheWriter(composite_key)` on its output.

A third cache point at the final `ColorConvert(display)` output is **not** added: the cost of
ColorConvert is small compared to its inputs and the working-space composite is the more
useful artefact (export reads it directly — §A.7).

### A.7 Export integration

`compile_export` (in `render/compiler.rs`) currently rebuilds the full graph at mip=0. With the
render cache, the same `compile()` flow used by the viewport already covers export, *because*:

- If the viewport hasn't yet rendered at mip=0 (almost always the case for large images), the
  layer-prefix and composite caches at mip=0 are cold, and the export pipeline runs all stages
  end-to-end — same cost as today.
- If they're warm (e.g. user dragged-to-fit a small image), export reads from the composite
  cache and skips everything before `Compose`. Export is then almost an I/O-only operation.

Implementation:

```rust
pub fn compile_export(doc: &Document, config: &CompileConfig, sink: Stage) -> ExecGraph {
    let req = RenderRequest {
        viewport: full_tile_range(config),
        mip_level: 0,
        up_to: None,
    };
    compile(doc, &req, config, sink)
}
```

unchanged externally; the cache-aware `compile()` does the rest.

User-visible UI: in the Export dialog (existing `ExportDialog`), add a "Prebake" checkbox
(default on). When ticked, controller fires `compile_export(...)` to the *internal* sink first
(a no-op consumer just to populate cache), then triggers the real export. For images that
fit in cache budget this makes Export → Save a near-instant operation. Wire via a new
`Action::PrebakeExport` that returns when `composite_key` hits.

### A.8 Undo/redo — wire up alongside the cache

Undo/redo logic exists in `pixors-document/src/history.rs::History` (`push`, `undo`, `redo`,
`can_undo`, `can_redo`). What is missing in Phase 10: **no UI surface and no controller
wiring**. Ship that in Phase 11 together with the cache, since the cache is what makes undo
worth shipping (otherwise every undo would re-run the whole pipeline).

Concrete tasks:

1. **Controller actions** in `pixors-desktop/src/controller/`:
   - `Msg::Undo` → calls `session.history.undo(&mut session.document)`. On `Some(label)`, bump
     `session.transient.redraw_seq` and call `run_render(session_id, mip, range)`. On `None`,
     no-op.
   - `Msg::Redo` → symmetric with `history.redo`.
2. **Menu items** in `components/menu_bar.rs`: Edit → Undo (Ctrl+Z) / Redo (Ctrl+Shift+Z).
   Enable/disable from `session.history.can_undo()` / `can_redo()`.
3. **Keyboard shortcuts** in `app.rs` `subscription` keyboard branch — emit `Msg::Undo` /
   `Msg::Redo`.
4. **Visual feedback**: toast or status-bar pill showing the action that was undone/redone
   (label comes back from `history.undo()`). Existing `Pill` widget covers it.

Why this works with the cache without extra code:

- Pre-mutation state was the source of cache entries until the mutation. They are *still on
  disk* — undo re-derives a `current_key` that matches a still-existing cache, so the compiler
  short-circuits to `CacheReader` and the render is instant.
- Redo: identical, the post-mutation key is also cached (we wrote it during the original
  apply).
- Deep history: once the user undoes past the MipN slot cap, older keys have been LRU-evicted
  and that one undo recomputes — see §A.1.1. The redraw still completes; only one step in the
  chain is slow.

Edge case: `RemoveTransform` then redo. The mutation's `apply` removes the transform, `undo`
re-inserts the same `Transform` (`id` preserved). Because `RenderKey` is hashed from
`params_hash` and `op` discriminant (not from `transform.id`), redo hits the cache. Regression
test required (§8.2).

Recordable filter: `Mutation::recordable()` already exists; preview-only changes (slider drag
ticks) should *not* be pushed. Today only the commit path calls `history.push` — verify in
`controller/filters.rs` and `controller/layers.rs` that drag-tick mutations bypass commit.

### A.9 Cache lifecycle and limits

- **Size budget**: `RenderCache::budget_bytes`. Default 2 GiB. Add a settings entry in
  `pixors-desktop` (Preferences dialog stub OK for v1 — read from env var
  `PIXORS_RENDER_CACHE_BUDGET_MB` for now).
- **Per-key budget**: none — eviction is per-key, atomic.
- **Eviction trigger**: after every `get_or_create` that bumps total usage over budget,
  evict oldest keys until under.
- **Manual reset**: new menu item `View → Clear cache` calls
  `session.transient.render_cache.reset()`. Implement as `cleanup()` + recreate.
- **Crash recovery**: cache dir is always under `std::env::temp_dir()/pixors/session_*`. On
  startup any leftover `session_*` dirs whose PID is not in `/proc` (Linux) or whose mtime is
  older than 24h (cross-platform fallback) are deleted. Add to `pixors-desktop/src/main.rs`
  before app start.

### A.10 What the cache does *not* do

- It does not cache GPU buffers. Tiles are written as CPU bytes (post-Download). GPU compose
  followed by a write means an extra Download — accepted because the alternative (keeping
  GPU-resident caches) needs lifetime tracking we don't want to build yet.
- It does not deduplicate identical tiles across keys. A blur with radius 5 and another graph
  that happens to produce the same bytes get two cache entries.
- It does not survive across sessions. If you close and reopen the same file, decoding runs
  again from source. Persistent caching is a backlog item ("library photo cache" — fits
  naturally inside Library workspace once that exists).

---

## 4. Workstream B — Compositor blend modes

### B.1 Enum extension

`pixors-engine/src/common/blend.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Source,                  // existing, "replace"
    Over,                    // existing, alias of Normal in compose context
    // New in Phase 11:
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    ColorDodge,
    ColorBurn,
    Difference,
    Exclusion,
}

impl BlendMode {
    pub fn stable_id(self) -> u8 { /* explicit assignment, NEVER reuse */ }
    pub fn label(self) -> &'static str { /* "Normal" / "Multiply" / … */ }
    pub fn all() -> &'static [BlendMode] { /* for the UI dropdown */ }
}
```

`stable_id` is consumed by `RenderKey`.

### B.2 CPU implementation

In `pixors-ops/src/processor/compose.rs::alpha_over_f32`, replace the current 3-arm match with
a full table. The math (all operate on straight-alpha RGBA, premultiply is performed inline
by the alpha-over wrapper):

```text
// `a` = bottom (result), `b` = top (incoming pixel), all components in [0,1]
fn separable_blend(mode, a_rgb, b_rgb) -> f32 per channel:
  match mode:
    Normal | Over | Source: b           // (Source short-circuits alpha — handled by caller)
    Multiply:   a * b
    Screen:     1 - (1-a)*(1-b)
    Overlay:    if a < 0.5 then 2*a*b else 1 - 2*(1-a)*(1-b)
    HardLight:  Overlay with arguments swapped
    SoftLight:  Pegtop variant — (1-2*b)*a^2 + 2*b*a
    ColorDodge: if b == 1 then 1 else min(1, a / (1-b))
    ColorBurn:  if b == 0 then 0 else 1 - min(1, (1-a) / b)
    Difference: |a - b|
    Exclusion:  a + b - 2*a*b
```

Compositing with alpha (Porter-Duff "over" outer envelope, separable inner colour formula):

```
let a_top = top[3] * opacity_top
let a_bot = bot[3]
let a_out = a_top + a_bot * (1 - a_top)
let c_blended = separable_blend(mode, bot_rgb, top_rgb)   // both in [0,1] straight
// Photoshop-style: when both alphas are 1, just c_blended. Otherwise:
let c_out = ( (1-a_bot) * top_rgb * a_top
              + (1-a_top) * bot_rgb * a_bot
              + a_top * a_bot * c_blended ) / a_out
```

Tests in `pixors-ops/src/processor/compose.rs` (a new `#[cfg(test)] mod tests`): for each mode,
hand-coded reference values for 4 input pairs (0,0), (0,1), (1,0), (1,1), and one mid value.
Compare with epsilon `1e-5` (f32 path).

### B.3 GPU implementation

`pixors-shader/shaders/compose.slang`: extract the per-pixel work into

```slang
float3 apply_blend(uint mode, float3 a, float3 b) {
    // switch table identical to CPU, branch on `mode`
}
```

`ComposeParams` (in `pixors-shader/shaders/lib/params.slang` or
`pixors-shader/src/kernel/compose.rs` — wherever the kernel struct lives, grep
`ComposeParamsKernel::new`) gains a `mode: u32`. The `Compose` Rust stage currently passes one
`opacity_b` per pairwise dispatch — extend the dispatch loop to pass `blend_modes[port].stable_id() as u32`.

GPU tests: golden image comparisons. The existing `pixors-shader/tests/` directory (create if
missing) holds 64×64 reference tiles per mode generated by the CPU path; the GPU dispatch must
match within 1/255 (= 0.0039) per channel.

### B.4 UI

Blend dropdown lives at the **top of the layers panel** (above the layer list, alongside the
opacity slider — Photoshop-style header), not inline per layer row. It targets the currently
selected layer and binds to `selected_layer.blend.mode`. When no layer is selected, both
controls are disabled.

`pixors-desktop/src/components/layers_panel.rs` (search for `BlendMode::Normal` to find the
current site): replace the hard-coded 1-item dropdown with
`BlendMode::all().iter().map(|m| (m.label(), *m))` and move it into the panel header row. The
mutation `SetLayerBlend` already exists and already `needs_recompile()` — nothing new there.

### B.5 Documentation

`docs/ARCHITECTURE.md` §6: replace the Normal-only line. Add a one-line entry per mode in
`docs/KNOWN_BUGS.md` removing any "blend modes missing" note.

---

## 5. Workstream C — Decoder breadth

### C.1 JPEG hardening (already exists, currently RAM-bound)

`pixors-image/src/jpeg/mod.rs::open_stream`:

Current behaviour: `decoder.decode()` returns the *entire* decoded RGB plane (`Vec<u8>` of
`w*h*bpp` bytes) in RAM before the first `drain()` call. For a 60MP CMYK JPEG that's 240 MiB
held even though we only need one tile-row at a time downstream.

Fix: split decoding from emission.

`zune_jpeg` exposes incremental decoding via `decode_into(&mut [u8])` — but only for the
whole buffer, not row-by-row. There's no public partial-decode API. Two paths:

| Path | Decision |
|---|---|
| Switch to `mozjpeg-sys` for true per-MCU row decoding | reject — adds a C dependency in a phase already adding 3 codecs. |
| Keep `zune_jpeg`, *spawn* the full decode on `prepare_ingest()`'s worker thread (it's already in a thread) into an `Arc<[u8]>`, then drain rows lazily | **accept** — we always decoded fully anyway; the only gain we want is "don't block the UI thread" which we already get because ingest runs off the main thread. |

So the only actual change for JPEG: nothing in `jpeg/mod.rs`, just verify the decode is on
the ingest worker thread (it is — `Dispatcher::run_graph` spawns it). Hardening = adding a
regression test that decodes a 5000×4000 CMYK and validates `prepare_ingest()` returns within
2× the time of a same-size RGB file (so we catch a future regression).

### C.2 WEBP — animated frames

`pixors-image/src/webp/mod.rs` currently treats WEBP as a single page (`pages: vec![…]`). The
`webp` crate already exposes `AnimDecoder` (`webp::AnimDecoder::new(&data).decode()` returns a
`AnimFrameIter`). Wire each frame as a page (analogous to multi-page TIFF). `PageInfo::delay_ms`
already exists and the multi-page ingest loop (`pixors-document/src/ingest.rs` lines 60–80)
already iterates `0..num_pages` creating one layer per page. No changes needed in
`pixors-document`.

Decision: animated WEBP plays nicely with the layer-per-page abstraction even though
"frames as layers" is semantically wrong. Phase 11 ships them as layers because:

- That's the path of least resistance and matches multi-page TIFF.
- A proper "timeline" UI for animated formats is unscoped before any video features.
- Visibility toggles let the user inspect each frame.

Document this in the Library v1 EXIF panel: a `Frames: N` field on detect.

### C.3 AVIF

New crate dep: `libavif-sys` is heavy. Pick `ravif` for encode-only or `libavif`/`avif-decode`
for read. We need only decode. Use `image-avif` (pure Rust, currently maintained as
`avif-decode = "1.0"` — verify on crates.io at implementation time, otherwise fallback to
`libavif-image` C bindings).

`pixors-image/src/avif/mod.rs` (new):

```rust
pub struct AvifDecoder;
impl ImageDecoder for AvifDecoder {
    fn probe(&self, path: &Path) -> Result<bool> { /* extension match avif|avifs */ }
    fn decode(&self, path: &Path) -> Result<ImageDescriptor> { /* parse, expose primary item dims and bit depth (10/12 supported) */ }
    fn open_stream(&self, path: &Path, page: usize) -> Result<Box<dyn PageStream>> { /* decode primary image to RGBA, then row-iterate like jpeg */ }
}
```

ImageDescriptor.bit_depth must be set correctly (10 or 12). Output PixelFormat:
`PixelFormat::Rgba16` for >8-bit AVIF (introduce if not present — grep
`pixors-color/src/common/pixel/rgba.rs` to confirm). ColorSpace defaults to `Rec.2020 PQ` if
the AVIF NCLX tags say so, else `sRGB`. Match in
`pixors-engine/src/common/color/detect.rs`.

### C.4 EXR

Crate: `exr = "1.x"` (well-maintained, pure Rust, no C deps).

`pixors-image/src/exr/mod.rs` (new). EXR is naturally tiled and f32; map directly:

- `PixelFormat::RgbaF32` (add if missing; `Rgba<f32>` Pixel impl needed)
- `ColorSpace::Linear` ("linear sRGB primaries, no transfer") — already exists via
  `ColorSpace::with_primaries(SRGB, TransferFn::Linear)`. Verify.
- The decoder emits scanlines as f32 RGBA. `ImageStreamSource` already handles arbitrary
  PixelFormat; the GPU `ColorConvert` already supports f32 inputs via the F32 codec in
  `pixors-shader/shaders/lib/codecs.slang`. Verify.

### C.5 Multi-page TIFF

Already infrastructure-supported in `ingest.rs` (page loop). Verify by opening a multi-page
TIFF (test fixture: a 4-page CMYK from `pixors-image/tests/data/` if it exists; otherwise
generate via `tiff::encoder` round-trip). Acceptance: each page appears as a Layer in the
panel, each writes its own subdir under `session_*/layer_{id}/`.

The Phase 10 invariant "the rest of the pipeline is unchanged" holds; no changes.

### C.6 Decoder registry

The current `open_image` (lines 148–179 of `pixors-image/src/image.rs`) is a hard-coded chain
of probes. With 2 new decoders it becomes 6. Refactor to:

```rust
// pixors-image/src/registry.rs (new)
pub struct DecoderRegistry {
    decoders: Vec<Arc<dyn ImageDecoder>>,
}
impl DecoderRegistry {
    pub fn standard() -> Self {
        Self { decoders: vec![
            Arc::new(png::PngDecoder), Arc::new(tiff::TiffDecoder),
            Arc::new(jpeg::JpegDecoder), Arc::new(webp::WebPDecoder),
            Arc::new(avif::AvifDecoder), Arc::new(exr::ExrDecoder),
        ] }
    }
    pub fn open(&self, path: &Path) -> Result<Image, Error> {
        for d in &self.decoders {
            if d.probe(path)? { return Ok(Image { desc: d.decode(path)?, decoder: d.clone(), path: path.into() }); }
        }
        Err(Error::internal("unsupported image format"))
    }
}
pub fn open_image(path: impl AsRef<Path>) -> Result<Image, Error> {
    DecoderRegistry::standard().open(path.as_ref())
}
```

`Image::decoder` was already `Arc<dyn ImageDecoder>`. Probe-first (cheap) then decode (heavy).
Move the "try decode and check error" anti-pattern out — each decoder now opts in via `probe`.

### C.7 Format-feature matrix

| Format | Read | Multi-page/frame | Alpha | Bit depth | Color space | Cache fingerprint |
|---|---|---|---|---|---|---|
| PNG       | ✓ existing | apng | ✓ | 8/16 | sRGB/ICC | path+mtime+size |
| TIFF      | ✓ existing | pages | ✓ | 8/16/32 | various, ICC | path+mtime+size |
| JPEG      | ✓ harden    | n/a    | n/a (alpha-drop) | 8 | sRGB/CMYK/Lab via JFIF | path+mtime+size |
| WEBP      | ✓ extend    | animation as pages | ✓ | 8 | sRGB | path+mtime+size |
| AVIF      | new         | n/a (track v1) | ✓ | 8/10/12 | NCLX → space | path+mtime+size |
| EXR       | new         | multi-part = pages | ✓ | 16/32 | linear | path+mtime+size |

HEIC/HEIF/CR3/NEF/ARW are explicitly Phase 12/13.

---

## 6. Workstream D — Thumbnails

### D.1 Thumbnail = MIP-N composite

We already produce mips during decode (`MipDownsample`). A thumbnail is the composite of all
visible layers at the smallest mip where the long edge is ≥ thumbnail size (target 256 px).
Algorithm:

```
fn thumbnail_mip(img_w, img_h, target_px) -> u32 {
    let mut mip = 0;
    while max(img_w >> mip, img_h >> mip) > target_px * 2 { mip += 1; }
    mip
}
```

Generating one is exactly `compile_export`-shaped except with `mip_level = thumbnail_mip(...)`
and the sink being a "save to disk PNG" stage. With the Smart Render Cache from §A, the
composite at that mip is already cached after a normal viewport view that touched it. The only
new pipeline component is a one-shot "PNG-encode and write to a path" consumer; the existing
`PngEncoderV2` (`pixors-image/src/sink/png_encoder_v2.rs`) handles tile-by-tile encoding and
already does this for export.

### D.2 Thumbnail cache layout

Distinct from `RenderCache` — thumbnails are *long-lived* and *per-file*, not per-session.

`~/.cache/pixors/thumbs/<sha-of-path-and-mtime>.png` (XDG on Linux; equivalent on mac/win).
256px or 512px on the long edge, lossless PNG.

Reading: `pixors-document::library::thumb_cache::read(path)` → `Option<image bytes>`.
Writing: `…::write(path, bytes)`. Both pure file ops, no pipeline.

### D.3 New action: `GenerateThumbnail`

Lives in `pixors-document::action::actions::generate_thumbnail`. Mode: `Background`.
Lifecycle:

1. Open file via `open_image` (cheap — header parse only, no decode).
2. Check thumb_cache; if present, return it.
3. Run an ingest-like pipeline at the chosen mip directly to the thumb_cache PNG path,
   bypassing any session: this is the only pipeline in the codebase that doesn't have a Tab.
   The dispatcher's per-session locking machinery is bypassed by setting `session_id = None`
   in `run_graph` (already supported).
4. Emit `PipelineEvent::Done` carries the thumb path; Library picks it up.

### D.3.1 Layer-panel thumbnail

Once the thumbnail exists, render it inside every row of the Layer Editor's layers panel
(replacing the current solid-colour swatch). Per-layer thumbnail = the same composite mip
result, but masked to that single layer (i.e. compile with only that layer visible, mip-N).
Reuse the existing `GenerateThumbnail` action; pass a `LayerThumbRequest { session, layer_id }`
variant so the cache key is layer-scoped, then store the decoded `image::Handle` on the
layer's UI state (not on the document — UI cache).

Refresh trigger: any mutation with `needs_recompile() == true` invalidates the affected
layer's thumb. Use `Transient::redraw_seq` plus a per-layer version counter:
`Transient::layer_thumb_versions: HashMap<NodeId, u64>` bumped on mutation; the panel widget
re-requests when the stored version differs from the displayed handle's version.

Layout: 48×48 thumbnail on the left of each row, name + opacity-indicator on the right.

### D.4 EXIF thumbnail short-circuit

Most cameras embed a small JPEG thumbnail in EXIF (`ExifTag::PreviewImage` or the IFD1 chain
of TIFF/RAW). `pixors-image/src/exif.rs` already parses EXIF for display. Add
`fn embedded_thumbnail(metadata: &[Metadata]) -> Option<Vec<u8>>` that returns the embedded
JPEG bytes; `GenerateThumbnail` uses it when present, skipping the entire decode pipeline.
Cuts a 24MP photo's thumbnail generation from ~150ms to ~3ms.

---

## 7. Workstream E — Library workspace v1

> **Note**: detailed UI spec for the Library workspace will be authored separately by the user
> and merged into the docs. The sections below capture only the data/state/action contracts
> the rest of the codebase depends on. UI layout, widgets, keyboard map, and visual design are
> placeholders here and may be superseded by the dedicated doc.

### E.1 Workspace concept

The desktop has been "Layer Editor only" so far. Introduce a `Workspace` enum on `App`:

```rust
// pixors-desktop/src/app.rs
pub enum Workspace {
    LayerEditor,
    Library,
}
```

Top-level menu/sidebar item switches between them. Each workspace owns its own page in
`pixors-desktop/src/pages/`:

- `pages/layer_editor.rs` — *all existing UI moves here*. (Today's content of `app.rs` view code
  is split between `app.rs::view()` and component widgets. Wrap it.)
- `pages/library.rs` — new.

The `App` struct still owns one `EditorState` (sessions are global), but most Library state is
its own struct (`LibraryState` — current directory, sort order, selection, rating filter).

### E.2 Library state

```rust
// pixors-desktop/src/library/state.rs
pub struct LibraryState {
    pub current_dir: PathBuf,
    pub entries: Vec<LibraryEntry>,         // sorted; populated by directory scan
    pub selected: Option<usize>,
    pub thumb_cache_hits: HashMap<PathBuf, ImageHandle>,    // in-RAM decode of thumb PNG
    pub pending_thumb_jobs: HashSet<PathBuf>,               // in-flight GenerateThumbnail
    pub filter: LibraryFilter,
}

pub struct LibraryEntry {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: SystemTime,
    pub kind: LibraryEntryKind,   // SupportedImage{format} | UnsupportedFile | Directory
    pub sidecar_rating: Option<u8>,        // 0..=5
    pub sidecar_flag: Option<Flag>,        // Pick | Reject | None
    pub exif_summary: Option<ExifSummary>, // make/model/lens/ISO/shutter/aperture/focal
}

pub enum Flag { Pick, Reject }
```

Directory scan is a worker thread: walk the dir non-recursively, build `entries`, lazy-load
EXIF (cheap) and lazy-request thumbnails. Sidecar files: see E.4.

### E.3 UI: grid

`pixors-desktop/src/library/grid.rs` — Iced `responsive` widget. Tile layout:

```
+--------+ +--------+ +--------+
|        | |        | |        |
|  thumb | |  thumb | |  thumb |
|  256px | |        | |        |
|        | |        | |        |
+--------+ +--------+ +--------+
 name.jpg   IMG_001   sunset
 ★★★★☆       ✓Pick      ✗Reject
```

Click → select. Double-click → fire `OpenFile` (existing action), switch workspace to
LayerEditor. Right-click → context menu (Open, Open in new tab, Rate 1..5, Flag Pick/Reject,
Reveal in file manager).

Keyboard: arrow keys move selection, `1..5` set rating, `P` Pick, `X` Reject, `Enter` opens.

### E.4 Rating + flag storage

XMP sidecar files (`<filename>.xmp`). XMP is XML; for v1 we write a minimal valid sidecar:

```xml
<x:xmpmeta xmlns:x='adobe:ns:meta/'>
  <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>
    <rdf:Description xmlns:xmp='http://ns.adobe.com/xap/1.0/'
                     xmlns:lr='http://ns.adobe.com/lightroom/1.0/'
                     xmp:Rating='4' lr:Pick='1'/>
  </rdf:RDF>
</x:xmpmeta>
```

Lightroom-compatible. Read: lightweight regex on the two attributes. Don't bring in a full XML
parser — we only ever write our own and tolerate read failures.

`pixors-document::library::sidecar::{read,write}`. Round-trip tests required.

EXIF-embedded ratings (some cameras write to `XMP::Rating` directly into the file) are read on
the fly; writes always go to sidecar.

### E.5 EXIF/IPTC summary panel

Right-side panel of the Library workspace, populated when a single entry is selected.

```
File: IMG_4521.cr3
Make: Canon                Model: EOS R5
Lens: RF 24-70mm F2.8
ISO: 800   1/250 s   f/4.0   45mm
Date: 2026-03-12 14:23:11
Dimensions: 8192 × 5464   Color: sRGB
```

All fields come from existing `pixors-image::exif::Metadata` enum, no new parsing. IPTC is
a simple extension to the EXIF parser (`pixors-image/src/exif.rs::iptc` module — add for v1
behind the same `Metadata` enum, expose Caption / Headline / Byline keys).

EXIF/IPTC *write* (the roadmap "EXIF/IPTC write + XMP sidecar" entry) is **deferred**: only
sidecar writes for rating/flag are in this phase.

### E.6 Smart collections — deferred

The roadmap explicitly says "Smart collections deferred to a later Library pass." Honour that;
no filter beyond rating threshold and Pick/Reject in v1.

### E.7 No file index DB in v1

Just walk the directory on entry. A SQLite index (so the Library can search across the disk)
is in the backlog. For v1, scrolling among hundreds of files in one folder is fine.

---

## 8. Cross-cutting tasks

### 8.1 CLAUDE.md and ARCHITECTURE.md

CLAUDE.md is out-of-date already (it describes the old `pixors-document/src/state/` layout that
doesn't exist; current layout is `document/`, `mutation/`, `session.rs`, `render/`). Update as
part of this phase:

- New `pixors-engine/src/cache/render_cache.rs` entry in the Key-Files table.
- Update the `pixors-image` table with `avif`, `exr`, `registry.rs`.
- Update the `pixors-desktop` table with `pages/library.rs`, `library/`.
- Replace any reference to the old `state/` paths with the actual paths.

Same for `docs/ARCHITECTURE.md` — the §3 ascii diagram needs a cache box, §5 needs the
`render_cache.rs` row.

### 8.2 Tests

| Test | Crate | Covers |
|---|---|---|
| `render_cache::key_stability_across_versions` | `pixors-engine` | Adding a transform invalidates only that key chain |
| `render_cache::source_fingerprint_changes_on_mtime` | `pixors-engine` | |
| `render_cache::evict_oldest` | `pixors-engine` | |
| `compile::layer_compiler_hit` | `pixors-document` | After two runs with same params, second graph has the source replaced with `CacheReader(prefix_key)` |
| `compile::layer_compiler_miss_after_param_change` | `pixors-document` | |
| `compile::composite_cache_hit_skips_layers` | `pixors-document` | |
| `compose::blend_modes::*` | `pixors-ops` | per-mode reference values |
| `compose::gpu_matches_cpu_within_epsilon` | `pixors-shader` | golden 64×64 |
| `webp::animated_pages` | `pixors-image` | 3-frame WEBP yields 3 PageInfo |
| `avif::primary_item_decode` | `pixors-image` | 8/10/12-bit AVIF round-trips bytes |
| `exr::scanline_emit` | `pixors-image` | |
| `library::xmp_roundtrip` | `pixors-document` | |
| `library::embedded_thumbnail` | `pixors-document` | RAW JPEG thumb extracted directly |

CI: `cargo test --workspace` already runs on every PR. The GPU tests need a CI runner with a
GPU — currently we don't have one. Mark GPU tests `#[ignore]` and ensure they run locally
via `cargo test -- --ignored gpu`.

### 8.3 Settings / preferences

Phase 11 adds two user-tunable knobs (render cache budget, thumb size). No preferences UI
exists yet; expose as env vars for now (`PIXORS_RENDER_CACHE_BUDGET_MB`,
`PIXORS_THUMB_SIZE_PX`), document in `README.md`.

---

## 9. Acceptance criteria & order of merge

Merge order is dictated by what unblocks what. Each row is one PR.

| # | PR | Acceptance |
|---|---|---|
| 1 | `feat(engine): render-cache infrastructure` (A.2–A.4) | Unit tests for `RenderKey`, dual-pool `RenderCache::{get_or_create,try_hit}`, RAM-LRU auto-evict on read in `DiskCache`. Not yet wired. |
| 2 | `feat(document): Compile trait + cache-aware compiler` (A.5–A.6) | `Compile` impls on `LayerNode`, `Transform`, `Operation`. Driver shrinks to ~30 lines. Second identical render shows `CacheReader` substitution in tracing log. All existing tests still pass. |
| 2b | `feat(desktop): undo/redo wiring` (A.8) | Ctrl+Z / Ctrl+Shift+Z + menu items work; undo of a slider commit is instant; deep undo past slot cap recomputes once. |
| 3 | `feat(ops): full blend mode set` (B) | Layers panel dropdown shows 10 modes; CPU + GPU give matching outputs per the test suite. |
| 4 | `feat(image): decoder registry + WEBP animation` (C.2, C.6) | A 4-frame animated WEBP opens as 4 layers. |
| 5 | `feat(image): AVIF decoder` (C.3) | An 8-bit AVIF and a 10-bit AVIF both open, the latter as RgbaF16 working. |
| 6 | `feat(image): EXR decoder` (C.4) | An ACES OpenEXR opens with `Linear` color space and round-trips visibly correct. |
| 7 | `feat(document): GenerateThumbnail action + thumb cache` (D) | Calling the action twice for the same file does not redecode. EXIF-embedded thumbs are picked up. |
| 8 | `feat(desktop): Library workspace v1` (E) | Open a folder, see thumbnails appear progressively, double-click opens, rate via keyboard, sidecar XMP round-trips with Lightroom. |
| 9 | `chore(docs): Phase 11 wrap` | CLAUDE.md / ARCHITECTURE.md updated; ROADMAP.md gains a "✓ Complete — Phase 11" block. |

PRs 3, 4, 5, 6 are independent of each other once PR 2 has landed (they touch different
files). Library (PR 8) can start in parallel with PRs 4–7 if a dev is available — its only
hard dep is PR 7.

---

## 10. Out of scope / explicitly deferred

These were considered and pushed out of Phase 11:

- **Persistent render cache across sessions** → **Phase 14** (Darkroom workspace). Non-destructive
  ops + stable source fingerprints give the best hit rate; that's where it earns its keep.
- **GPU-resident cache** → **Phase 12** (alongside RAW v1). Needs per-tile lifetime/ref-count on
  `Arc<GpuBuffer>` so eviction cannot race with in-flight dispatches.
- **Cache stats panel** (hits/misses, RAM/disk per pool) → **Phase 12**, shipped with the
  GPU-resident cache so the new tier is observable from day one.
- **Animated playback / timeline UI** for WEBP / APNG / etc. → **not planned**. Animated formats
  decode as a layer stack and that is the entire product affordance. Recorded in the
  "Not on the roadmap" footer of `ROADMAP.md`.
- **Luminosity / Color / Hue / Saturation blend modes.** Need Lab compositor variant; revisit
  with Darkroom (Phase 14).
- **HEIC/HEIF.** Phase 13.
- **EXIF/IPTC writing into the file itself.** Only sidecar in Phase 11 (roadmap aligns).
- **Smart collections / face detection.** Backlog.

---

## Appendix A — Quick checklist for the implementing agent

When you begin, in order:

1. Read this entire doc. Cross-check current code against §2 — the architecture overview is
   the source of truth for "what exists today."
2. Ship PR #1 first (render-cache infrastructure, no wiring yet).
3. After PR #2 lands and the compiler is cache-aware, **measure** a previously-slow case
   (a blur slider drag) and add the number to `docs/KNOWN_BUGS.md` or to a `BENCHMARKS.md` so
   future regressions are visible.
4. Keep the PRs small. The roadmap's pattern (one phase = one big PR) does not survive
   Phase 11 because the cache touches everything; split for review.
5. Re-run `cargo fmt --all` and `cargo clippy --workspace -- -D warnings` before every push.
   The workspace `Cargo.toml` denies a strict lint set — see CLAUDE.md.
6. Add a Phase 11 completion block to `ROADMAP.md` at the top, mirroring the Phase 9 / Phase
   10 blocks.

---

## Appendix B — One-paragraph summary

Phase 11 widens read-format support (JPEG hardening, WEBP animated, AVIF, EXR, multi-page
TIFF surfaced as layers), completes the separable blend-mode set on both CPU and GPU
compositors, and stands up a v1 Library workspace with folder browsing, thumbnails (with EXIF
short-circuit), Lightroom-compatible XMP sidecar ratings/flags, and a basic EXIF summary
panel. Underpinning all of it is a new Smart Render Cache: a session-scoped disk-backed cache
keyed by `(layer source fingerprint, transform-prefix params)` with a sibling cache at the
post-compose stage, transparently consulted by a refactored `LayerCompiler` that prunes the
upstream graph on hit. This makes repeated slider applies cheap, makes undo/redo near-instant,
and lets Export reuse work done by the viewport.
