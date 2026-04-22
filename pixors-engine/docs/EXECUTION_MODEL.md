# Execution Model

How the high-level operation graph becomes concrete work. Pixors uses a **two-level IR**: the user builds a high-level DAG of semantic operations; the engine compiles it down to a lowered plan of primitive steps (transfers, kernel dispatches, meta materializations). Work units, schedulers, and error reporting all operate on the lowered plan.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Pipeline | Two-level IR — high-level op graph → lowered execution plan |
| Lowered node kinds | `UploadTile`, `DownloadTile`, `DispatchCpu`, `DispatchGpu`, `MaterializeMeta`, `AllocTile`, `ReleaseTile`, `Barrier` |
| Work unit | One node in the lowered plan = one concrete dispatch / transfer |
| Transfers | Explicit nodes in the lowered plan (not implicit) |
| Fusion | Performed at plan-compilation time (compile-time, not runtime) |
| Shader cache | Keyed by the fused kernel's op-chain + constant-folded params + input types |
| Progress events | Fire per tile (per lowered node completion) |
| Error traceability | Every lowered node back-references its originating high-level op |
| Cancellation | Delivered at the work-unit boundary (see [D35](DECISIONS.md#d35--cooperative-cancellation-per-work-unit)) |

## Two-level IR

### Level 1 — High-level op graph

What the user builds via the `Context` API. Nodes are semantic operations (`brightness`, `gaussian_blur`, `save_png`). Edges are typed `ValueId`s. Described in [OPERATION_GRAPH](OPERATION_GRAPH.md).

### Level 2 — Lowered execution plan

What the engine actually runs. Nodes are primitive steps:

| Kind | Meaning |
|---|---|
| `AllocTile(engine, tile_id)` | Reserve storage for a tile on a specific engine |
| `UploadTile(tile, CPU→GPU)` | Copy tile data between engines (direction specified) |
| `DownloadTile(tile, GPU→CPU)` | Same, reverse direction |
| `Swap(tile, CPU→DISK \| DISK→CPU)` | Paging to/from DISK swap |
| `DispatchCpu(kernel, inputs, outputs)` | Run a compiled CPU kernel on one output tile |
| `DispatchGpu(kernel, inputs, outputs)` | Vulkan compute dispatch for one output tile |
| `MaterializeMeta(meta_id)` | Compute a `Meta` / `MetaBuffer` value (reductions from images, pure-math chains) |
| `ReleaseTile(tile)` | Release a handle (triggers drop-flush per [D14](DECISIONS.md#d14--engine-owns-allocation-shared-via-arc-drop-flushes)) |
| `Barrier(set_of_nodes)` | Ordering constraint: downstream nodes wait |

Edges in the lowered plan are dependencies: data flow, memory residency, barriers.

### Why two levels

- High-level stays **declarative** — easy to reason about, easy to build, cheap to analyze
- Lowered stays **concrete** — scheduler sees exactly what to dispatch, where, and in what order
- Cross-cutting concerns (fusion, transfer insertion, cancellation) happen **once** in the lowering pass rather than being scattered through op implementations
- Optimization passes (fusion, DCE, constant folding) transform the lowered plan before execution

Cost: error handling spans two IRs. Every lowered node carries a back-reference (`source_op_id`) so errors report at the high-level.

## Compilation pipeline

Invoked by `ctx.run()`:

```
High-level graph
    │
    ▼
[ 1. Type-check & validate ]       ← inputs match signatures, no cycles
    │
    ▼
[ 2. Dead-code elimination ]       ← drop nodes with no reachable output sinks
    │
    ▼
[ 3. Constant folding ]            ← literal Meta nodes inlined
    │
    ▼
[ 4. MIP scaling resolution ]      ← each op's scale_for_mip runs, producing per-level params
    │
    ▼
[ 5. Fusion planning ]             ← group fusible op chains into fused kernels
    │
    ▼
[ 6. Engine assignment ]           ← per fused group, choose CPU or GPU
    │
    ▼
[ 7. Work-unit expansion ]         ← per (fused group, output tile) emit a lowered-plan node
    │
    ▼
[ 8. Transfer insertion ]          ← wherever residency changes, insert Upload/Download/Swap nodes
    │
    ▼
[ 9. Dependency graph build ]      ← link nodes by data-flow and residency
    │
    ▼
Lowered execution plan  ──▶  Scheduler
```

Steps 1–8 are the "compile" phase. Step 9 produces the DAG the scheduler consumes.

## Work units

A **work unit** is one node in the lowered plan. Concretely:

- One `DispatchCpu` / `DispatchGpu` for a single output tile (possibly executing a fused kernel covering multiple high-level ops)
- One `UploadTile` / `DownloadTile` / `Swap` for a single tile
- One `MaterializeMeta` for a single meta value
- One `AllocTile` / `ReleaseTile` for a single tile

"Work unit" is not a high-level concept — it is whatever the lowered plan says is one scheduled step.

## Fusion mechanics

Performed in step 5 of compilation.

### Grouping rule

Walk the high-level graph in topological order. Consecutive ops fuse into one group when:

- Same target engine (CPU or GPU)
- Compatible tile topology (same MIP level, compatible neighborhood — see below)
- No fork in the intermediate (if the intermediate is consumed by 2+ downstream ops, it must materialize; fusion stops at the fork)

"Compatible neighborhood" for per-pixel ops is trivially `(0,0,0,0)`. For two neighborhood ops in sequence, the combined neighborhood is the sum of reaches. Fusion of neighborhood-with-neighborhood is an optional aggressive mode — the baseline fuses only chains of per-pixel ops between neighborhood boundaries.

### Kernel emission

For each fused group, emit one kernel:

- **CPU**: a concatenated inline function — each op's body, taking the previous op's output as its input, all in `f32` compute registers
- **GPU**: a concatenated SPIR-V body — same idea, generated at build-compile time when possible, at runtime for dynamic fusion patterns

Both avoid intermediate memory: read `f16` → compute in `f32` through all ops → write `f16` once ([D8](DECISIONS.md#d8--storage-f16-compute-f32)).

### Compile-time preference

Fusion runs at plan-compile time inside `ctx.run()`. Runtime dispatch is simple: hand the compiled kernel to the engine and go. Keeping fusion out of the dispatch hot path is a deliberate cost/benefit choice — dispatches happen per tile per frame; compilation runs once per job.

## Shader / kernel cache

Compiling a fused kernel is expensive (SPIR-V generation, Vulkan pipeline creation). The result is cached.

### Key

```
CacheKey = {
    op_chain:        Vec<OpId>,             // which ops, in order
    constant_params: Vec<Bytes>,            // folded literal params (bytes)
    engine:          CpuOrGpu,
    input_types:     Vec<ValueType>,
    neighborhood:    Neighborhood,
}
```

Dynamic Meta params do **not** appear in the key — they are bound at dispatch time as uniform / storage buffers. Only values folded into the compiled kernel (literals) are part of the key.

### Tiers

- **In-memory LRU cache** (mandatory) — lives for the `Context` session, holds compiled pipelines
- **On-disk cache** (optional, future) — persists across sessions; shader blobs hashed by the cache key

Disk cache is a future optimization. In-memory is enough for interactive editing within a session.

## Dependency graph

The lowered plan is a DAG. Edges:

- **Data** — a kernel's output tile feeds another kernel
- **Residency** — a `DispatchGpu` for tile `T` depends on an `UploadTile` for `T` and its required neighbors
- **Barriers** — explicit ordering nodes insert when needed (e.g. before a reduction that must see all contributors)

The scheduler topologically orders the plan, but chooses the next ready node by **priority** (see [SCHEDULER](SCHEDULER.md) _(TBD)_). Ready = all prerequisites complete.

### Priority propagation

Priority is inherited: a lowered node has the priority of the high-level op it belongs to, raised by the priority of any viewport-interactive descendant. This is how a slider change pulls its own work units to the front.

When two downstream ops have very different neighborhood requirements (one needs 8 tiles, another needs 50), they share upstream dependencies. The scheduler runs the larger-neighborhood op only when its broader set of tiles is ready — a priority/readiness interaction the scheduler manages, not the execution model.

## Transfers as first-class nodes

Every data movement is a visible node:

- Need input on GPU? Scheduler dispatches the `UploadTile` before the `DispatchGpu`
- Output needs to reach disk? `DownloadTile` (GPU→CPU) then `Swap` (CPU→DISK) chain
- Same tile on both CPU and GPU (mirrored) is valid — both allocations are tracked, and writes mark the corresponding copies dirty

Visibility lets the scheduler batch, reorder, and cancel transfers independently of compute dispatches.

## Meta materialization

`Meta` and `MetaBuffer` values are computed by `MaterializeMeta` nodes:

- Constant-folded literals — no node; the value is baked into the consuming kernels
- Pure-math chains (Meta → Meta) — CPU-side computation, a single `MaterializeMeta` per node
- Image-reducing ops (`histogram`, `image_statistics`) — execute as `DispatchCpu` or `DispatchGpu` with a reduction kernel; their output is a `MaterializeMeta` downstream consumer

Downstream kernels that consume the meta wait for materialization before dispatch.

## Progress events

Fired at **per-tile granularity**:

```
event: TileCompleted {
    job_id,
    high_level_op_id,
    tile_id,
    engine,
    duration_ms,
}
```

Aggregation (overall job progress) is computed from tile events by the caller or a helper. This lets viewport refresh trigger tile-by-tile — the UI doesn't wait for the whole job to paint a partial result.

Other events:

- `JobStarted` — job compiled, ready to dispatch
- `JobCompleted(result)` — all outputs materialized
- `JobCancelled` — token set, remaining work units skipped
- `JobFailed(error)` — see below

## Error handling across IR levels

Every lowered node carries `source_op_id: Option<HighLevelOpId>`. On error:

1. Lowered node returns `Err(LoweredError)`
2. Error is wrapped with `source_op_id` and any identifying input/output tile IDs
3. Scheduler decides the response:
   - **Transfer failure** ([D5](DECISIONS.md#d5--whole-job-failure-on-tile-failure) exception) → replan on alternate engine if the op has a kernel there; otherwise fail the job
   - **Compute failure** → fail the job ([D5](DECISIONS.md#d5--whole-job-failure-on-tile-failure))
   - **Allocation failure** → attempt cascaded downtransfer ([D16](DECISIONS.md#d16--capacity-auto-negotiated-overflow-cascades-downward)); fail the job if exhausted
4. Error reported to user via `JobFailed(ExecutionError { source_op, tile_id, cause })`

The user sees a failure in terms of the high-level op they asked for, not in terms of internal lowered nodes. The lowered info is present in the error for debugging.

## Cancellation in the lowered plan

The scheduler checks the job's cancellation token before dispatching each work unit ([D35](DECISIONS.md#d35--cooperative-cancellation-per-work-unit)). If set:

- `AllocTile` / `ReleaseTile` still run (cleanup must not leak)
- All other node kinds are skipped
- No error is produced; the job transitions to `Cancelled`
- `JobCancelled` event fires once

In-flight dispatches and transfers run to completion (no Vulkan mid-dispatch abort). The tail latency is bounded by one work unit's duration.

## Relations

- [OPERATION_GRAPH](OPERATION_GRAPH.md) — the high-level IR this model compiles
- [OPERATIONS](OPERATIONS.md) — per-op declarations feeding compilation
- [TILE_SYSTEM](TILE_SYSTEM.md) — tile identity used throughout lowered plan
- [STORAGE_ENGINES](STORAGE_ENGINES.md) — transfer semantics
- [SCHEDULER](SCHEDULER.md) — consumes the lowered plan, drives dispatch _(TBD)_
