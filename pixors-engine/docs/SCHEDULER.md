# Scheduler

Consumes the lowered execution plan and drives dispatch. Responsible for priority arbitration, engine routing, Vulkan queue management, cancellation delivery, and viewport integration. Deliberately simple in Phase 1 — more sophisticated behaviors (backpressure, predictive prefetch, engine-load-aware routing) are deferred.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Priority representation | Numeric (integer); no named tiers in the scheduler |
| Tiebreaker | FIFO by job arrival time |
| Engine routing | Static — op has GPU kernel → GPU; otherwise CPU |
| Worker model | Async pools (tokio-style); one or more |
| Vulkan queues | Compute + transfer queues, multiple frames in flight while resources allow |
| Preemption | Cooperative, at work-unit boundary ([D35](DECISIONS.md#d35--cooperative-cancellation-per-work-unit)) |
| Viewport integration | Callback-driven, asynchronous |
| Prefetch | Deferred — not in MVP |
| Meta materialization | Competes in the same main queue as image work units |

## Priority

A priority is a plain integer attached to a job, inherited by every lowered node it produces. The scheduler takes the ready node with the highest priority; within a priority level, FIFO by job arrival breaks ties.

No named "ViewportInteractive / MipGeneration / Background" enum at the scheduler level. The host (or higher-level components like the viewport tracker) picks numbers, and the scheduler does not care what the numbers mean. A convention for typical priority values may emerge, but it is not a hard type in the scheduling layer.

### Priority propagation through the lowered plan

Every lowered node inherits the priority of the high-level op it belongs to. A lowered node with multiple high-level sources (fused group) takes the **max** priority of its sources — the fastest thing in the group wins.

Priority can be **raised** after the plan is built. The viewport-tracking layer may tell the scheduler: "tiles currently visible: raise their priority to X". The scheduler walks the relevant nodes and updates their priorities. Reordering of the ready queue happens automatically on the next dequeue.

## Ready queue

The scheduler maintains a set of lowered-plan nodes whose dependencies are satisfied. On each scheduling tick:

1. Find ready nodes (all prerequisites complete)
2. Select the node with the highest priority
3. Tiebreak by job arrival time (oldest first)
4. Tiebreak further by any stable secondary (e.g. node ID)
5. Dispatch

Simpler than a priority queue data structure: a bucket-per-priority structure (BTreeMap or array of FIFOs) is sufficient and matches the numeric-priority design.

## Engine routing — static rule

Phase 1 rule:

> If the op implements `GpuKernel`, route to GPU. Otherwise, route to CPU.

No runtime load-balancing, no GPU-queue-depth awareness, no transfer-cost estimation. If GPU capacity is exceeded, the eviction cascade ([D16](DECISIONS.md#d16--capacity-auto-negotiated-overflow-cascades-downward)) handles it. If a GPU transfer fails, the fallback mechanism ([D5](DECISIONS.md#d5--whole-job-failure-on-tile-failure)) redirects to CPU if a CPU kernel exists.

Adaptive routing (backpressure, load-aware) lands later when profiling shows the static rule costs real performance on realistic workloads.

## Job lifecycle

States:

| State | Meaning |
|---|---|
| `Building` | User is still adding nodes via `Context` |
| `Compiling` | `ctx.run()` called; lowering pass running |
| `Scheduled` | Lowered plan handed to scheduler; some work units may be dispatched |
| `Running` | Work units dispatching and completing |
| `Completed` | All output nodes finished successfully |
| `Cancelled` | Cancellation token set; remaining work units skipped |
| `Failed(error)` | Unrecoverable error; see error handling in [EXECUTION_MODEL](EXECUTION_MODEL.md) |

Transitions fire events ([D46](DECISIONS.md#d46--progress-events-fire-per-tile)). A `Context` tracks its current jobs; users receive `JobHandle`s back from `ctx.run()`.

## Worker model

Async pools — tokio-style runtime with multiple executors:

- **CPU worker pool** — runs `DispatchCpu` kernels and `MaterializeMeta` for CPU-side math. Size = `num_cpus` by default, configurable
- **Transfer worker pool** — orchestrates `UploadTile` / `DownloadTile` / `Swap` operations; awaits Vulkan fences and OS async I/O completions
- **GPU submit worker** — single thread for `vkQueueSubmit` calls (Vulkan queues are not thread-safe for submit); work units prepare command buffers in parallel, submit funnels through this worker

Whether these are literally separate tokio runtimes or one runtime with multiple executor handles is an implementation detail. The invariant is: async tasks represent work units; awaitable futures represent completion.

## Vulkan queue strategy

### Queues

- **Compute queue** — one, for `DispatchGpu` work units (compute shader dispatches)
- **Transfer queue** — one, for `UploadTile` / `DownloadTile` (and `Swap` via staging buffers)

If the hardware only exposes a single queue family with combined capabilities, fall back to one queue and serialize submits.

### Multi-frame in flight

Pixors always has multiple work units "in flight" on the GPU as long as resources are available:

- Command buffers are allocated per work unit (or pooled and reset)
- Descriptor sets are allocated per dispatch (or pooled)
- Staging buffers for transfers are pooled; when pool is exhausted, the new transfer waits
- Each in-flight dispatch has a dedicated fence

"Frames in flight" count is bounded by resource availability (staging pool depth, command buffer pool, VRAM headroom) and by a configurable cap (`max_concurrent_gpu_work_units`, default e.g. 8).

### Synchronization

- Cross-queue dependencies use Vulkan semaphores (upload complete → compute runnable)
- Within a queue, pipeline barriers handle memory visibility between dispatches
- CPU ↔ GPU completion uses fences polled by the transfer worker

No frame graphs, no render graphs — Pixors is not a rendering engine. Direct semaphore/fence wiring per work unit is enough for compute + transfer workloads.

## Preemption and cancellation

Cancellation is cooperative at the work-unit boundary ([D35](DECISIONS.md#d35--cooperative-cancellation-per-work-unit)):

1. New job arrives with a higher priority, or an existing job's cancel token is set
2. The scheduler, on its next dequeue, checks the cancel token of the owning job
3. If cancelled, the work unit is not dispatched; scheduler moves on
4. In-flight dispatches run to completion (no Vulkan mid-shader abort, no CPU kernel interruption)
5. Once all in-flight work units of the cancelled job drain, the job transitions to `Cancelled`

Because work units are per-tile, tail latency is bounded by the slowest single work unit — typically a few milliseconds on GPU, potentially longer on CPU for a large reduction.

## Viewport integration

Asynchronous, callback-based.

### Host-side responsibility

The host (UI layer, future GUI, or the `Context` user in library mode) knows what the viewport is. It tells the scheduler:

```
fn set_viewport(image_id, mip_level, rect) -> ()
```

This is a normal async call; the scheduler updates its internal state and reorders accordingly.

### What the scheduler does with this

Given a viewport `(image, mip, rect)`:

1. Compute the set of tile IDs covering the viewport at the given MIP level
2. Raise the priority of any in-flight or pending lowered nodes whose output targets those tiles
3. Pin those tiles on GPU (for the current MIP level and for MIP 0 during refinement — see [MIP_PYRAMID](MIP_PYRAMID.md) refinement flow)
4. Unpin tiles that left the viewport

### Event direction

Scheduler does not poll the host. Host is expected to call `set_viewport` whenever the viewport changes (pan/zoom). Per-tile completion events ([D46](DECISIONS.md#d46--progress-events-fire-per-tile)) flow the other direction, via callback to the host.

## Prefetch

Deferred past Phase 1.

When added, "simple prefetch": tiles adjacent to the current viewport are scheduled at a low priority as background work units (load from disk / compute MIP / upload to GPU as appropriate). No pan-direction prediction, no velocity-based lead. This matches the "whenever resources are idle, start filling out coverage" pattern of normal progressive loading.

## Meta materialization in the main queue

`MaterializeMeta` lowered nodes compete in the same ready queue as image-tile work units. They do not have a separate pool.

Consequences:
- A reduction (histogram, statistics) is just another work unit
- Its priority is inherited from its source op, same rules as any other
- A downstream op waiting for Meta materialization blocks on that ready node like any other data dependency
- No deadlock: the lowered plan is a DAG, so eventually all Meta nodes become ready in topological order

## What the scheduler does not do

Deliberately out of scope, Phase 1:

- Backpressure based on GPU queue depth
- Load-aware CPU↔GPU routing
- Transfer-cost-aware engine selection
- Pan-direction predictive prefetch
- Battery / thermal awareness
- Per-user QoS negotiation / energy budgets

All of these are candidates for later phases when real workloads motivate the complexity.

## Relations

- [EXECUTION_MODEL](EXECUTION_MODEL.md) — produces the lowered plan the scheduler consumes
- [STORAGE_ENGINES](STORAGE_ENGINES.md) — pinning, eviction, transfer primitives
- [OPERATIONS](OPERATIONS.md) — kernel availability drives static routing
- [MIP_PYRAMID](MIP_PYRAMID.md) — refinement flow defines priority ordering for viewport tiles
- [API](API.md) — user-facing viewport callback and job handle surfaces _(TBD)_
