# Decisions Log

Cross-cutting architectural decisions. Each entry records **what** was decided, **why**, and **what it forecloses**. Reopen only with new evidence.

## D1 — Strict three-tier storage: `DISK <-> CPU <-> GPU`

**Decision**: No direct DISK↔GPU transfer path. Data moves disk↔RAM and RAM↔VRAM only.

**Why**: Memory-mapped file I/O on modern OSes reaches the same throughput as round-tripping through the CPU. Direct DISK↔GPU adds a second code path with no real performance win.

**Forecloses**: GPU-resident direct file I/O (e.g. DirectStorage-style). Revisit only if profiling shows a real gap for a real workload.

## D2 — Rectangular tile neighborhoods only

**Decision**: `Neighborhood { left, right, top, bottom }`. No circular, diagonal, or arbitrary structuring elements.

**Why**: Virtually all production image ops use rectangular kernels. Irregular neighborhoods complicate work-unit formation with negligible benefit.

**Forecloses**: Some morphology ops with exotic structuring elements (handle via bounding-box over-read + in-kernel masking if ever needed).

## D3 — Typed `ValueId` index, not UUID

**Decision**: Value handles are a typed newtype index (`ValueId(u32)` or similar) into a graph-local table, not a UUID.

**Why**: Smaller, faster lookups, no allocation, cache-friendly. UUID adds no value when the graph is process-local.

## D4 — Operation capabilities are opt-in per engine

**Decision**: An `Operation` declares only the kernels it actually provides (CPU, GPU, or both). No forced empty implementations.

**Why**: An op that is inherently CPU-only (complex branching, tiny-image hot paths) should not be required to produce a GPU shader stub.

## D5 — Whole-job failure on tile failure

**Decision**: If a tile-level computation fails, the entire job fails. No partial results.

**Why**: Tile-level failures in neighborhood ops (blur, convolution) produce visibly broken results. Honest failure > silent corruption.

**Exception**: **transfer** failures (CPU↔GPU) fall back to running the work unit on the other engine. A failed upload does not fail the job if the CPU kernel exists.

## D6 — API surface first; bindings, MCP, CLI, GUI later

**Decision**: The programmatic Rust API is the first deliverable. Everything else (MCP server, Python/JS bindings, CLI, GUI) is built on top and deferred.

**Why**: Scope control. Forces the core API to be ergonomic on its own terms.

## D7 — Working space: ACEScg (AP1, D60, linear)

**Decision**: All internal pixel data is in ACEScg linear, premultiplied RGBA, `f16` storage, interleaved.

**Why**: ACEScg is the de facto VFX/cinema wide-gamut working space. Linear avoids hidden gamma bugs. Premultiplied alpha is correct for compositing. `f16` halves memory vs `f32` with acceptable color precision. Interleaved matches GPU buffer layouts.

**Related**: See [DATA_MODEL](DATA_MODEL.md) for the full specification. D60 vs D65 noted and accepted — chromatic adaptation (Bradford CAT) handles conversion to/from D65 spaces.

## D8 — Storage `f16`, compute `f32`

**Decision**: Kernels read `f16` → compute in `f32` → write `f16`. No `f32` buffers in normal pipeline.

**Why**: Precision where it matters (compute), compactness where it matters (memory). Avoids precision erosion in accumulative ops without doubling memory cost.

## D9 — DISK layout planar, CPU/GPU layout interleaved

**Decision**: Tile data is interleaved RGBA in RAM and VRAM. On-disk format follows convention of the target file format (planar for TIFF/EXR when supported). Layout conversion only at the DISK boundary.

**Why**: Interleaved matches Vulkan storage-buffer layout and most per-pixel ops. Planar on disk matches format conventions and compresses better per channel.

## D10 — GPU: storage buffers only, no texture units

**Decision**: GPU-side tile data lives in `VkBuffer` with `STORAGE_BUFFER` usage. Hardware bilinear sampling, clamping, and filtering are simulated in shader code when needed.

**Why**: Unifies the transfer model (one buffer type), avoids texture format/size limits, and simplifies the tile abstraction. The cost of in-shader bilinear (4-tap + mix) is acceptable.

**Forecloses**: Hardware texture filtering paths. Revisit only if a specific op is bound on that cost.

## D11 — HDR tone mapping deferred

**Decision**: Tone mapping for SDR display is not in the initial scope. Values above 1.0 are stored and processed normally but not mapped for display.

**Why**: Pixors is first a processing engine, not a display pipeline. Tone mapping belongs to the eventual viewport layer.

## D12 — ICC profiles: hardcoded fast path + generic fallback

**Decision**: Hardcode the common color spaces (sRGB, Rec.709, Rec.2020, Adobe RGB, Display-P3, ProPhoto, ACES2065-1, ACEScg, Linear sRGB). Fall back to a generic ICC engine (e.g. `lcms2` / `qcms`) for anything else.

**Why**: Covers >95% of real-world inputs with zero dependency cost on the hot path. The ICC fallback keeps correctness for arbitrary profiles.

## D13 — Tile size: 256×256

**Decision**: Default tile is 256×256 pixels. RGBA `f16` interleaved → 512 KiB per tile.

**Why**: Small enough to keep neighborhood overhead low, large enough to amortize dispatch overhead, supported on every relevant GPU, yields a manageable tile count (≈1024 for 8K images).

**Forecloses**: Nothing hard. Tile size may become configurable per image if a real workload demands it.

## D14 — Engine owns allocation; shared via `Arc`; drop flushes

**Decision**: Each storage engine owns its allocations. Consumers hold `Arc<Handle>`. On last-drop, dirty non-transient handles flush to the next tier; transient handles are discarded.

**Why**: Balances safe sharing (multiple graph nodes can reference the same tile) with deterministic release (flushing on drop prevents data loss).

## D15 — All transfers async via futures/events

**Decision**: Every transfer, upload, download, and compute dispatch returns an awaitable future. Completion events propagate uniformly.

**Why**: Matches the core principle of never blocking the caller. Unifies GPU fences, OS async I/O, and CPU work behind one API.

## D16 — Capacity: auto-negotiated; overflow cascades downward

**Decision**: Each engine queries its tier for available budget (`VK_EXT_memory_budget` for GPU, OS sysinfo for CPU, configured cap for DISK swap). Overflow evicts to the next slower tier: GPU → CPU → DISK swap. DISK swap full = job fails.

**Why**: Adaptive to the user's real hardware without configuration. The cascade direction respects speed asymmetry (never evict toward faster tier).

## D17 — LRU eviction with pinning

**Decision**: Eviction uses LRU over non-pinned tiles only. Tiles in the current viewport are pinned on GPU so pan/zoom never stalls on re-upload.

**Why**: LRU is the right baseline for access-driven caches. Pinning is required for interactive responsiveness where LRU alone would cause thrashing.

## D18 — Batched, priority-sorted transfer submission

**Decision**: Work units and their prerequisite transfers are sorted by priority and submitted in descending order. Compatible GPU transfers batch into single `vkQueueSubmit` calls.

**Why**: Priority ordering realizes QoS. Batching amortizes submit overhead, which matters at high work-unit counts.

## D19 — DISK is two sub-engines: `FileStorage` + `SwapStorage`

**Decision**: Source and output files (read-only after load; written once at save) live in `FileStorage`. Paging scratch area for evicted tiles lives in `SwapStorage`. Both expose the storage-engine interface but have different write semantics.

**Why**: Source files must never be mutated accidentally. Swap needs random writes and a different failure model. Separating them keeps invariants clear.

## D20 — One `VkBuffer` per tile

**Decision**: GPU-side tiles are each a dedicated `VkBuffer` (`STORAGE_BUFFER | TRANSFER_SRC | TRANSFER_DST`). Neighborhood ops bind central + neighbor tiles as separate descriptor bindings.

**Why**: Tile count is manageable for target image sizes. Avoids suballocation complexity. Simple descriptor management.

**Forecloses**: Nothing. If descriptor-update overhead shows up in profiling, switch to VMA suballocation later (handle becomes `(buffer, offset, size)`).

## D21 — Cold state compression: LZ4

**Decision**: Committed editor states that are not currently hot are compressed with LZ4 (patent-free). Hot state stays uncompressed.

**Why**: LZ4 has excellent decompress speed and a clean licensing story. Lossless is required for history correctness. Image tiles in `f16` compress enough to make the trade worthwhile.

**Forecloses**: Nothing critical. Can layer ZSTD or block formats later for specific cases if memory pressure demands a better ratio.

## D22 — Boundary tiles: padded to full size; valid region in metadata

**Decision**: Every tile buffer is exactly 256×256. Images whose dimensions are not multiples of 256 have partial tiles at their right/bottom edge, with the unused region zero-padded. Real extent recorded in `valid_region: (width, height)` on the tile.

**Why**: Uniform buffer size simplifies allocation, SIMD, and GPU dispatch. Zero padding is consistent with premultiplied alpha (contributes nothing to blends). Metadata cost is trivial.

**Forecloses**: Nothing. Variable-size edge tiles would add branching on every dispatch with no real benefit.

## D23 — `TileId` is `{ image_id, mip_level, x, y }`; MIP level is part of identity

**Decision**: A tile is identified by the struct `{ image_id, mip_level, x, y }`. Each MIP level owns its own tile grid. Tiles at different MIP levels are distinct entities even when they overlap the same image region.

**Why**: Operations are MIP-aware. A tile is an executable unit — it must carry its MIP level. Separate grids per MIP keep all tiles at a uniform 256×256, which preserves the allocation and dispatch simplicity of [D13](DECISIONS.md#d13--tile-size-256256).

## D24 — Dirty tracking: one flag per tile

**Decision**: Each tile has a single dirty flag. No sub-tile dirty regions.

**Why**: Most ops touch the whole tile; transfer granularity is per-tile; bookkeeping for sub-tile dirty masks rarely pays off. Revisit only if interactive edit workloads show real overhead from full-tile flush.

## D25 — Per-image tiles stored as a flat `Vec` with linear offsets

**Decision**: Tiles for an image are stored in a flat `Vec<Tile>` indexed via per-MIP offset tables. `(image_id, mip, x, y)` is bijective with a linear offset.

**Why**: Dense storage is fine for the common case. The bijection is cheap to compute in either direction. Avoids `HashMap` hashing cost on every lookup.

**Forecloses**: Nothing. Sparse gigapixel workflows can adopt a sparse backing later without changing the `TileId` design.

## D26 — MIP generation: box filter default; higher-quality filters optional

**Decision**: The default MIP-pyramid generation filter is a box filter (average 2×2). Higher-quality filters (bilinear, Lanczos, Mitchell) are offered as optional alternatives for save-time or user-requested regeneration.

**Why**: Box is the cheapest correct choice in a linear (ACEScg) working space — no gamma correction required because data is already linear. Good enough for preview. Higher-quality filters stay available when quality matters more than speed.

## D27 — MIP 0 is canonical; higher levels are derived

**Decision**: MIP 0 is the single source of truth after any edit. Higher MIP levels are derived from MIP 0 via the generation filter, with lazy regeneration.

**Why**: Prevents MIP levels from drifting out of consistency. Simplifies invalidation (edit MIP 0 → mark higher levels stale → refill on demand). Observers always agree on MIP 0, may transiently disagree on higher levels during refinement.

## D28 — Minimum pyramid resolution: 64×64

**Decision**: The MIP pyramid stops generating at the level where either image dimension would drop below 64×64.

**Why**: Below 64×64 the image is a thumbnail and further downsampling has no useful signal. A 64×64 level is still large enough to use as a thumbnail itself. Storage of the full pyramid down to 64×64 is bounded to at most ~33% overhead on top of MIP 0.

## D29 — Every operation must be MIP-aware

**Decision**: Every operation is required to produce a sensible result at any MIP level. There is no `mips_aware` opt-out flag. Parameters expressed in pixels scale as `R / 2^n`; parameters that don't depend on resolution stay constant.

**Why**: The fast-preview → canonical refinement loop depends on MIP-level results being structurally consistent with MIP 0. A non-MIP-aware op would cause a visible jump when refinement completes, breaking the core UX promise. Worst case, an op that truly cannot scale falls back to rendering viewport tiles at MIP 0 only (skips fast preview), but never to producing a structurally wrong fast preview.

## D30 — Edit invalidates MIP 0 plus all covering higher-MIP tiles; lazy regeneration

**Decision**: When a MIP 0 tile is modified, the tile is marked dirty at MIP 0 and all covering higher-MIP tiles (one per level, via `(x >> n, y >> n)`) are marked stale. Regeneration happens lazily, driven by the scheduler during the refinement phases.

**Why**: `O(mip_max)` invalidation cost per edit is trivial. Lazy regeneration avoids doing work the user never asks to see. Aligned with the canonical-MIP-0 rule.

## D31 — Value types: `Image`, `Meta<T>`, `MetaBuffer<T>`

**Decision**: The graph produces and consumes exactly three value families: `Image` (tiled pyramid), `Meta<T>` (fixed-size POD, bindable as uniform), and `MetaBuffer<T>` (dynamic POD array, bindable as storage buffer). No other value categories.

**Why**: Covers every realistic image-processing intermediate with minimal type-system surface. Clean mapping to GPU resource kinds (storage buffer image, uniform buffer, storage buffer).

## D32 — Uniform dataflow: parameters are just upstream values

**Decision**: An operation's "parameters" are not a separate concept from its "inputs". Everything is an upstream `ValueId`. Literal Rust values passed at graph-construction time are automatically wrapped as a constant `Meta` node.

**Why**: Lets any configuration value come from upstream computation (auto-parameterization, adaptive pipelines). Removes a second-class mechanism. Constant-folding passes handle the static case at zero cost.

## D33 — Graph is a strict DAG; iteration is explicit node repetition

**Decision**: The operation graph is acyclic. Iterative algorithms are expressed by repeatedly adding nodes. Convergence loops live in host-language control flow, running sub-graphs as needed.

**Why**: Simplifies dependency analysis, fusion, and scheduling. Keeps every value with exactly one producer. Matches how users actually structure iterative image work.

## D34 — Fusion baseline: same engine + same tile topology → single kernel

**Decision**: Two consecutive ops fuse into one dispatched kernel if they run on the same compute engine (CPU or GPU) and share tile topology (same MIP level, compatible neighborhood). Forks, engine transitions, and neighborhood mismatches split fusion. Literal `Meta` constants fold into the compiled kernel.

**Why**: Captures the big wins (per-pixel chains, same-engine straight lines) with a simple analyzable rule. Leaves room to widen later when a real workload shows a bottleneck.

## D35 — Cooperative cancellation per work unit

**Decision**: Every job carries a cancellation token. Before dispatching each work unit, the scheduler checks the token and skips the dispatch if set. Work units already dispatched run to completion. In-flight transfers are not aborted.

**Why**: Matches the interactive editing UX — slider ticks generate a fresh job per tick, cancelling the previous. Per-work-unit granularity gives bounded tail latency without the cost of per-pixel aborts or mid-dispatch teardown. Output tiles remain in a valid state at all times (the previous committed version until the new job overwrites).

## D36 — Op kernels via separate CPU / GPU traits

**Decision**: CPU and GPU kernels are exposed via two distinct traits (`CpuKernel`, `GpuKernel`). An op implements whichever subset it supports. No stubs, no runtime "not implemented" errors. An op with no implementation at all fails to compile.

**Why**: Enforces [D4](DECISIONS.md#d4--operation-capabilities-are-opt-in-per-engine) statically. Keeps CPU-only ops (complex branching, small-image paths) free of GPU boilerplate. Type system ensures the scheduler never dispatches to a missing backend.

## D37 — MIP scaling is op-owned

**Decision**: Each operation is responsible for computing its own scaled parameters at a given MIP level. The engine does not rescale params automatically. Ops expose a step that transforms inputs `(inputs, mip) → scaled_inputs` before work-unit formation.

**Why**: Scaling rules are op-specific (Gaussian sigma does not scale linearly with pixel radius; color params do not scale at all). Centralizing in the engine would either be wrong or force a combinatorial tag system. The op is the only place that knows its own math.

## D38 — Boundary policy declared per op; no global default

**Decision**: Each op declares its own boundary policy (clamp / mirror / wrap / zero / transparent). No engine-wide default.

**Why**: The correct choice depends on op semantics (blur uses mirror, edge-detect uses clamp, tiling uses wrap). A global default would be wrong for most ops most of the time. Per-op declaration keeps the contract visible at the op definition site.

## D39 — Phase 1 MVP op set

**Decision**: Phase 1 ships with a defined MVP set:

- I/O: `load_png`, `save_png`
- Pixel: `brightness`, `contrast`, `gamma`, `gain`, `invert`, `color_matrix`, `premul`, `unpremul`
- Local: `gaussian_blur`, `box_blur`
- Geometric: `crop`
- Metadata: `histogram`, `image_statistics`, `find_interest_points`, `match_points`, `compute_transform`

**Why**: Covers per-pixel fusion cases, at least one neighborhood op (to exercise the work-unit machinery), at least one reduction op (to exercise Meta production), and enough to reproduce the end-to-end image-alignment example. Compositing and full geometry deferred to avoid ballooning Phase 1.

## D40 — Image format parsing via established crates; interest-point algorithms in-tree

**Decision**: Use existing Rust crates for PNG/JPEG/TIFF/EXR parsing. Implement ORB, brute-force Hamming matcher, and RANSAC-style transform estimators from scratch inside Pixors — no OpenCV or equivalent C++ FFI dependency.

**Why**: Format parsing is a solved problem with mature crates; reinventing it adds bugs and no value. Interest-point algorithms are small, self-contained, and bringing OpenCV adds massive transitive dependencies for a few hundred LoC of algorithm code. SIFT is patent-expired (2020) and ORB was always free.

## D41 — Two-level IR: high-level op graph → lowered execution plan

**Decision**: The engine has two IRs. The user builds a high-level DAG of semantic operations. `ctx.run()` compiles it into a lowered execution plan of primitive steps (transfers, kernel dispatches, meta materializations, alloc/release, barriers). Schedulers, fusion, transfer insertion, and work-unit formation all operate on the lowered plan.

**Why**: Separates what from how. Keeps the user-facing graph declarative and easy to reason about. Concentrates cross-cutting concerns (transfer planning, fusion, cancellation, priority) in one lowering pass. Accepted cost: errors must carry a back-reference from lowered nodes to high-level ops for reporting.

## D42 — Work unit = one lowered-plan node

**Decision**: A work unit is exactly one node in the lowered plan — one dispatch, one transfer, one meta materialization, one alloc/release. "Work unit" is not a high-level concept.

**Why**: Makes scheduling granularity match fusion outcome (a fused chain is one dispatch = one work unit). Aligns cancellation granularity ([D35](DECISIONS.md#d35--cooperative-cancellation-per-work-unit)) with the real unit of dispatch.

## D43 — Transfers are first-class nodes in the lowered plan

**Decision**: Every data movement — CPU↔GPU upload/download, CPU↔DISK swap — is an explicit node in the lowered plan, not an implicit side effect of dispatch.

**Why**: Makes transfers visible for scheduling, batching, cancellation, and error recovery. Enables transfer-failure fallback ([D5](DECISIONS.md#d5--whole-job-failure-on-tile-failure)) without tangling it inside dispatch logic.

## D44 — Fusion happens at plan-compile time

**Decision**: Kernel fusion (combining per-pixel op chains into a single dispatched kernel) runs during `ctx.run()` plan compilation. Runtime dispatch takes the compiled kernel and executes it directly.

**Why**: Compilation happens once per job; dispatch happens per tile per frame. Paying fusion cost up-front keeps the hot path simple. Also lets the fusion pass make global decisions the runtime couldn't.

## D45 — Compiled kernel cache keyed by op chain + folded params + input types

**Decision**: Compiled CPU kernels and GPU SPIR-V pipelines are cached in a session-wide LRU keyed by `(op_chain, constant_folded_params, engine, input_types, neighborhood)`. Dynamic (non-folded) Meta values are bound at dispatch time and are not part of the cache key. A disk-persisted cache is a future optimization.

**Why**: Compilation is expensive. Fused kernels for the same semantic chain with the same static shape recur across jobs. Dynamic Meta values are cheap to rebind; keeping them out of the key makes the cache actually hit in interactive sessions.

## D46 — Progress events fire per tile

**Decision**: Progress events fire at per-tile (per-lowered-node) granularity, not per-op or per-job. Aggregation to coarser levels is left to the caller.

**Why**: Per-tile events enable tile-by-tile viewport refresh. The UI can render partial results as they complete, matching the progressive-refinement UX promised by the MIP pyramid.

## D47 — Lowered nodes back-reference their source high-level op

**Decision**: Every node in the lowered plan carries `source_op_id` pointing back to the high-level op it was generated from (a fused node references the chain of source ops). Errors include this reference so failures report at the high-level-op level.

**Why**: Users think in terms of the ops they asked for ("blur failed"), not in terms of dispatches. Keeping the back-reference cheap and universal keeps error messages useful without complicating normal execution.

## D48 — Priority is a numeric value; no named tiers at the scheduling layer

**Decision**: The scheduler treats priority as a plain integer. Higher number = higher priority. No `ViewportInteractive / Background / Prefetch` enum in scheduling logic. Host layers and viewport tracking assign whatever numbers make sense; the scheduler just orders by them.

**Why**: Keeps the scheduler simple and policy-agnostic. Priority values can evolve without refactoring the scheduler. Naming conventions can emerge at the API or host layer without hardcoding into core.

## D49 — Ready queue tiebreaker: FIFO by job arrival

**Decision**: Within a priority level, the oldest job's work units dispatch first. Further ties broken by a stable secondary (e.g. node ID).

**Why**: Starvation-free, predictable, trivial to implement. Fancier schemes (locality, engine affinity) are future work.

## D50 — Engine routing is static: GPU kernel → GPU, else CPU

**Decision**: Phase 1 scheduler routes every op to GPU if a GPU kernel exists, otherwise to CPU. No dynamic load-balancing, no queue-depth awareness, no transfer-cost estimation.

**Why**: Simplest correct rule. Eviction cascade handles capacity pressure; transfer-failure fallback handles correctness. Dynamic routing lands when profiling proves the static rule is costing real performance.

## D51 — Worker model: async pools (tokio-style)

**Decision**: Scheduling runs on async executors. A CPU worker pool handles CPU dispatches and pure-math Meta nodes. A transfer worker pool awaits Vulkan fences and OS async I/O. A single GPU submit worker serializes `vkQueueSubmit` calls (queues are not thread-safe for submit). Whether implemented as one or several runtimes is an implementation choice.

**Why**: Fits the fully-asynchronous contract ([D15](DECISIONS.md#d15--all-transfers-async-via-futuresevents)). Async primitives make multi-frame-in-flight coordination natural. Separating pools by function prevents long CPU kernels from starving transfer completions.

## D52 — Vulkan: compute + transfer queues, multi-frame in flight

**Decision**: Use one compute queue for `DispatchGpu` and one transfer queue for uploads/downloads. Multiple work units in flight simultaneously while resources (command buffers, staging buffers, descriptor sets, VRAM headroom) allow. Hard cap configurable (default e.g. 8 concurrent GPU work units). Cross-queue dependencies via Vulkan semaphores; in-queue via pipeline barriers; CPU↔GPU completion via fences.

**Why**: Transfer queue in parallel with compute queue overlaps upload with compute, which is where most GPU-accelerated pipelines gain real throughput. Multi-frame in flight keeps the GPU busy instead of serializing on single-dispatch completion.

## D53 — Viewport integration is callback-driven

**Decision**: The host tells the scheduler about viewport changes through an async call (`set_viewport(image, mip, rect)`). The scheduler reacts by raising priorities and pinning tiles. Progress events flow back via callback. The scheduler does not poll the host.

**Why**: Keeps UI-specific concerns out of the engine. Usable identically from a GUI app and a library embedding. Matches the asynchronous / event-driven core design ([D15](DECISIONS.md#d15--all-transfers-async-via-futuresevents)).

## D54 — Prefetch deferred past Phase 1 MVP

**Decision**: No prefetch in Phase 1. When added (Phase 4 scheduler polish or later), the initial implementation is "simple prefetch": tiles adjacent to the viewport scheduled at low priority as idle-time work. No pan-direction prediction.

**Why**: Prefetch is an optimization on top of a correct interactive pipeline. Building it before the baseline is correct risks premature complexity. "Simple" scope is intentional — predictive prefetch is a research-grade problem not worth the complexity yet.

## D55 — Meta materialization competes in the main queue

**Decision**: `MaterializeMeta` lowered nodes share the main ready queue with image-tile work units. Their priority is inherited from their source op like any other node. No separate pool.

**Why**: Unified queue simplifies the scheduler. Meta dependencies block downstream consumers via normal dependency edges ([D43](DECISIONS.md#d43--transfers-are-first-class-nodes-in-the-lowered-plan)), no deadlock in a DAG. Separate pools would only matter if Meta ops showed up as a distinct bottleneck, which is not expected.

## Open questions

- Shader language — GLSL vs WGSL vs HLSL (leaning GLSL for Vulkan directness)
- Fusion codegen strategy — how a fused per-pixel chain emits a combined kernel
- Transfer failure retry protocol — which component drives the fallback replan (scheduler leaning)
- Swap compression threshold — when does a swap tile get LZ4-compressed vs left raw
- Save-time MIP embedding policy — which formats and under what default / user flag
