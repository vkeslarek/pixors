# Design Review — Round 1

Cross-cutting review of the design docs as of 2026-04-20. Identifies simplifications, holes, inconsistencies, and architectural risks. Items are labeled `S` (simplification), `B` (buraco / hole), `I` (inconsistency), `R` (risk). Priority tier at the bottom.

## Simplifications

### S1 — DISK `FileStorage` is not really a storage engine

`FileStorage` (introduced in [D19](DECISIONS.md#d19--disk-is-two-sub-engines-filestorage--swapstorage)) only provides load/save — it does not have the `allocate` / `read` / `write` / `transfer_to` shape of a real storage engine. It is effectively implemented by the `load_png` / `save_png` operations.

**Proposal**: Drop the "engine" framing for `FileStorage`. Keep the name for the file-I/O layer, but remove it from the storage-engine interface. Only `SwapStorage` is a DISK-tier storage engine proper.

**Impact**: Shrinks the storage-engine surface. No runtime behavior change.

### S2 — Transient flag on tile handles is redundant

[D14](DECISIONS.md#d14--engine-owns-allocation-shared-via-arc-drop-flushes) + a `transient: bool` on the handle (STORAGE_ENGINES.md) over-specify. A dropped tile with `dirty` set that belongs to committed editor state should flush; a dropped tile with no dirty state or not part of committed state should free. Committed-vs-intermediate is naturally tracked by the editor layer (when added), not by a boolean on the storage handle.

**Proposal**: Remove the `transient` flag. Flush rule becomes "`dirty && part_of_committed_state`". Until the editor layer exists, treat all tile drops as free (intermediates are the only thing producing tiles in Phase 1).

### S3 — `Meta<T>` vs `MetaBuffer<T>` can be reframed

The uniform-vs-storage-buffer GPU binding distinction is an engine concern, not a user concern. The split exists because Rust's type system differs for fixed-size POD vs dynamic POD arrays.

**Proposal**: Keep both types but rename to clarify intent:
- `Meta<T>` — fixed size, engine binds as uniform
- `MetaArray<T>` — dynamic length, engine binds as SSBO

Or: keep current names. Low-priority naming.

### S4 — Pinning API via RAII guard, not `pin()` / `unpin()` pair

Unbalanced `pin()` without matching `unpin()` leaks VRAM indefinitely.

**Proposal**: `let _guard = engine.pin(tile_id);` returns a `PinGuard` whose `Drop` calls `unpin`. Manual `unpin` remains available for edge cases.

**Impact**: Implementation detail of the storage-engine API.

### S5 — Fused node's source-op back-reference must be a list

[D47](DECISIONS.md#d47--lowered-nodes-back-reference-their-source-high-level-op) says "source_op_id" singular. A fused lowered node covers N source ops.

**Proposal**: `source_op_ids: SmallVec<HighLevelOpId; 4>`. Error reporting surfaces the first (or the one that triggered the failure if known).

## Holes

### B1 — Load pipeline is not specified end to end

Loading a u8 sRGB PNG requires: container decompress → per-pixel `u8→f32` → gamma decode → primaries matrix to AP1 → Bradford CAT (D65→D60) → premultiply alpha → pack to `f16`. Several passes per pixel. Currently no doc names where each step happens.

**Proposal**: `load_png` is a single composite op that internally lowers to a sequence of primitive steps: container decompress (CPU-bound, single worker), then a tiled `DispatchCpu` that runs the color pipeline per tile. Pipeline is fusible on CPU into a single per-pixel function.

**Target docs**: DATA_MODEL (add "load pipeline" section) + OPERATIONS (describe `load_png` / `save_png` internals).

### B2 — Gamut mapping policy on save is unspecified

Converting ACEScg → sRGB or ACEScg → Rec.709 loses out-of-gamut colors. Possible policies:

- **Hard clamp** — values above destination gamut clamp to `[0, 1]`; simple, can produce banding / hue shifts
- **Per-channel soft rolloff** — compressive tone response; preserves detail but shifts hue
- **Perceptual gamut mapping** (saturation preserved along hue lines) — higher quality, more code
- **ICC rendering intent** (Perceptual / Relative Colorimetric / Absolute) — standard pro workflow, but requires lcms2 engagement

**Proposal**: Phase 1 uses hard clamp by default, with a `GamutMappingMode` Meta parameter on `save_*` ops to opt into softer modes when implemented. Document the default clearly. Phase 2+ adds perceptual modes.

**Target doc**: DATA_MODEL + OPERATIONS.

### B3 — Saving to formats without alpha is unspecified

JPEG has no alpha channel. Current docs do not say what happens.

**Proposal** (three options, pick one):
- **A** — error if alpha is non-trivial (any pixel `α < 1`)
- **B** — flatten against configurable background color (default: black)
- **C** — flatten against black, no configuration

**Recommendation**: B (flatten against configurable background, default black). Matches user expectation of "save as JPEG".

**Target doc**: OPERATIONS `save_*` entries.

### B4 — Topology-changing ops are not declared

`crop`, `resize`, `rotate`, `warp` produce an `Image` with a different tile grid (potentially different dimensions). The lowering / fusion pass needs to know this to avoid crossing the boundary. Currently not declared.

**Proposal**: Add to the Op anatomy: `changes_topology: bool`. Fusion never crosses a topology boundary. The output is a new `Image` with its own `image_id` and tile grid.

**Target doc**: OPERATIONS (Op anatomy).

### B5 — MIP N tiles have two writers (fast preview + canonical composition)

The refinement flow ([MIP_PYRAMID](MIP_PYRAMID.md)) writes a MIP N tile first as fast-preview (direct op at MIP N), then overwrites it via composition from MIP 0 upward. In lowered-plan DAG terms, two nodes produce the same tile.

**Proposal**: Frame it as two separate lowered-plan output nodes targeting the same tile ID, with an ordering edge forcing the composition node to run after the fast-preview node. Document this explicitly in MIP_PYRAMID and EXECUTION_MODEL. Clarify that the invariant "every `ValueId` has exactly one producer" ([OPERATION_GRAPH](OPERATION_GRAPH.md)) applies to the high-level graph — the lowered plan permits targeted tile overwrites with explicit ordering.

### B6 — ICC profile handling for exotic profiles is per-pixel lcms2

If an image ships with a non-hardcoded ICC profile, D12 says to fall back to lcms2. Naively this means running lcms2 for every pixel — far too slow.

**Proposal**: On load, compile the ICC profile once into a compact runtime form (3D LUT, typically 33³ or 65³ samples, with trilinear interpolation at use). The compiled LUT is a Meta value consumed by the load pipeline. Apply LUT per pixel on CPU or GPU.

**Target doc**: DATA_MODEL (ICC fast path + LUT approach) + OPERATIONS (`load_*`).

### B7 — Multiple images in one `Context` is unspecified

Does a `Context` hold one image pipeline or many? Do multiple images share a single job on `ctx.run()`?

**Proposal**: A `Context` is a graph-builder that can hold arbitrarily many `ValueId`s. `ctx.run()` snapshots the **reachable-from-outputs** subgraph and launches one `Job`. Users wanting isolated scheduling or cancellation scopes create separate `Context`s.

**Target doc**: API (when written) + OPERATION_GRAPH (clarify scope of a single job).

### B8 — Graph mutation during an in-flight run is unspecified

User calls `ctx.brighten(...)` while a previous `ctx.run()` is still executing. What happens?

**Proposal**: `ctx.run()` snapshots the current graph and produces a `Job` with its own lowered plan. Subsequent `ctx.*` mutations extend the live graph in the `Context`; they do not affect the in-flight `Job`. The next `ctx.run()` snapshots the current graph again — typical for slider interaction.

Alternative considered: pause mutation during run. Rejected — it would require a sync API, contradicting the never-block contract.

**Target doc**: API + OPERATION_GRAPH (snapshot semantics).

### B9 — Reading `Meta` output is unspecified

After `ctx.run()` completes, how does the user read the value of `Meta<Transform>` or `MetaBuffer<KeyPoint>` on the host?

**Proposal**: `ValueId<Meta<T>>` supports:
- `ctx.read(id) -> Future<Result<T>>` — async read, resolves when the value has materialized
- `ctx.read_blocking(id) -> Result<T>` — for sync contexts

Similarly for `MetaArray<T>`.

`Image` values are not directly readable as bytes — they are tile-backed. Reading pixels uses a separate op (`ctx.read_pixels(image, rect)`).

**Target doc**: API + OPERATION_GRAPH.

### B10 — No Context reset / value drop

A long-lived `Context` accumulates `ValueId`s and graph nodes indefinitely. No way to reclaim memory.

**Proposal**:
- `ctx.drop_value(id)` — removes the value and (transitively) any nodes that become unreachable
- `ctx.reset()` — wipes the entire graph

After a `Job` completes, values that are no longer referenced are eligible for drop automatically (garbage collection on run boundaries).

**Target doc**: API.

### B11 — GPU device selection is unspecified

Which GPU does Pixors use on a multi-GPU system? Integrated vs discrete?

**Proposal**: Default to discrete GPU when present, falling back to integrated. `Context::with_device(...)` allows explicit selection. Enumeration via `Context::available_devices()`.

**Target doc**: API.

### B12 — Viewport pinning lacks a budget

Pinning all viewport tiles at MIP 0 for an 8K image = 1024 × 512 KiB = 512 MiB, plus neighbors + pyramid. A desktop GPU with 4 GiB VRAM can run out.

**Proposal**: Soft cap on pinned VRAM (e.g. 60% of VRAM). When exceeded:
1. Unpin the oldest pinned tile (least-recently-touched)
2. Emit a `PinBudgetExceeded` event so the host can react (e.g. UI may choose to lower MIP level)

Documented trade-off: pan might occasionally re-upload tiles if the viewport is enormous.

**Target doc**: STORAGE_ENGINES + SCHEDULER.

### B13 — Unified error type is not specified

Errors come from Vulkan, CPU kernels, I/O, allocation, ICC, lowering. No top-level enum defined.

**Proposal**: One `pixors::Error` enum with variants by category (`Vulkan`, `Io`, `Kernel`, `Allocation`, `FormatDecode`, `ColorManagement`, `Cancelled`). Every variant carries optional `source_op_id` where applicable. `From` impls for common upstream errors.

**Target doc**: API (when written).

### B14 — `ctx.run()` blocking behavior during compilation is unclear

Compilation (lowering, fusion, planning) happens synchronously before returning a `JobHandle`. For a large graph (thousands of nodes), this is non-trivial.

**Proposal**: Accept synchronous compilation. Measure it. If it becomes a UX issue in practice, move compilation onto a worker thread behind a second future (`JobHandle` becomes `Future<Result<Running>>` → `Running::progress()`). Do not design for the worst case until it appears.

**Target doc**: API, with a note about future async-compilation option.

### B15 — Algorithmic "no result" vs systemic failure is not distinguished

`find_interest_points` on a uniform black image: zero keypoints. Is that `Err(NoFeatures)` or `Ok(MetaBuffer::empty())`?

**Proposal**: Empty results are valid outputs. Only hard errors (allocation, kernel crash, cancellation) fail the job. Ops that care about thresholds (e.g. RANSAC failing to find a consensus) expose that via a typed Meta (`Meta<TransformResult { transform, quality, inliers }>`) rather than job failure.

**Target doc**: OPERATIONS.

### B16 — `Transform` type is referenced but not defined

`compute_transform` produces `Meta<Transform>` but `Transform` is not described.

**Proposal**: `Transform` is an enum:
- `Transform::Translation(Vec2)`
- `Transform::Affine(Mat2x3)`
- `Transform::Perspective(Mat3x3)`

Geometric ops accept `Meta<Transform>`. The enum is POD (tagged union of fixed-size payloads).

**Target doc**: OPERATIONS.

## Inconsistencies

### I1 — `Meta<T>` and Vulkan uniform limits

Vulkan `maxUniformBufferRange` is typically 64 KiB. Histograms (4 KiB) fit. Large `Meta<T>` types (close to or exceeding 64 KiB) should not bind as uniform — they go through SSBO.

**Proposal**: Document the 64 KiB threshold in DATA_MODEL (or OPERATION_GRAPH value-types section). Engine automatically promotes to SSBO when size exceeds the limit; the user does not need to choose `Meta` vs `MetaArray` based on size alone. Optionally: inspect `T`'s size at compile time with `const` evaluation and pick binding kind.

### I2 — "Every op must be MIP-aware" vs "op may declare a MIP level not useful"

[D29](DECISIONS.md#d29--every-operation-must-be-mip-aware) is absolute. OPERATIONS MIP-scaling section says an op can decide a MIP level is not usefully computable and fall back to "MIP 0 only".

**Proposal**: Clarify D29 wording: the contract is that **every op produces a sensible result at every MIP level it is asked to run**. The fallback ("I cannot produce a useful fast-preview here; run MIP 0 and let composition handle MIP N") is itself a MIP-aware decision, not an opt-out.

## Risks

### R1 — One `VkBuffer` per tile hits `maxMemoryAllocationCount` for very large images

[D20](DECISIONS.md#d20--one-vkbuffer-per-tile) acknowledges VMA suballocation as a later step. The inflection point arrives sooner than "gigapixel" suggests:

- NVIDIA drivers often allow 4096 allocations
- With MIP pyramid, an 8K image has ~1367 tiles — safe
- A 16K image (16384×16384) has ~5461 MIP-0 tiles alone — already over the limit
- Descriptor set updates per-dispatch become expensive at this scale even if allocations fit

**Recommendation**: Re-evaluate before Phase 3 completes. Have VMA suballocation in the back pocket. If Phase 1 testing exercises only small/medium images, this risk can defer. If Phase 1 explicitly targets large images, bring VMA forward.

### R2 — Fusion codegen complexity

Generating fused SPIR-V at plan-compile time is not trivial. Options:

- Write a minimal SPIR-V emitter in-tree — maximum control, maximum code
- Use `naga` — parse each op's source, splice IRs, emit SPIR-V
- Use `rspirv` — programmatic SPIR-V builder in Rust

**Recommendation**: Prototype with `rspirv` or `naga` before committing. Phase 1 can ship with fusion **disabled** (every op is a separate dispatch) and still be correct; fusion is a performance optimization. Re-prioritize once Phase 3 GPU work is underway.

### R3 — `tokio` dependency weight

"tokio-style async" ([D51](DECISIONS.md#d51--worker-model-async-pools-tokio-style)) does not mandate tokio itself. Alternatives:

- **tokio** — full-featured, large dep tree, widely used
- **smol** / **async-executor** — lighter, fewer features
- **futures + custom executor** — minimal, full control

**Recommendation**: Pick at Phase 1 implementation time. Leaning `smol` + `futures` for lightness, but tokio's ecosystem maturity (Vulkan fence integration, ergonomic timers) may be worth the weight. Open question.

## Priority tiers

### Tier 1 — must resolve before Phase 1 coding begins

- B1 (load pipeline) — blocks `load_png`
- B2 (gamut mapping) — blocks `save_*`
- B3 (save without alpha) — blocks `save_*` to JPEG
- B4 (topology-changing ops) — affects `crop` behavior in graph
- B5 (MIP double-writer invariant) — affects lowered-plan correctness
- I2 (D29 rewording) — removes ambiguity

### Tier 2 — resolve with API doc

- B7 (multiple images in Context)
- B8 (graph mutation during run)
- B9 (reading Meta values)
- B10 (Context reset / value drop)
- B13 (unified error type)
- B14 (`run()` blocking behavior)

### Tier 3 — apply cleanup now; no blocking

- S1 (drop FileStorage framing)
- S2 (drop transient flag)
- S4 (RAII pinning guard)
- S5 (back-reference list)

### Tier 4 — address when Phase 3 is in sight

- R1 (one VkBuffer per tile scaling)
- B11 (GPU device selection)
- B12 (pinning budget)
- R2 (fusion codegen)
- R3 (async runtime choice)
- S3 (`Meta` vs `MetaArray` naming)
- B6 (ICC LUT compilation)

### Tier 5 — wait for editor layer

- B16 (`Transform` type)
- B15 (algorithmic vs systemic failure) — partial, depends on editor policy

## Status

Document is the output of the first design-round review. No code yet. Design decisions in [DECISIONS](DECISIONS.md) remain valid where not explicitly superseded by items above. Items are proposals, not yet locked.
