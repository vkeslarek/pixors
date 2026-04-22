# Storage Engines

Three engines with strict transfer topology: `DISK <-> CPU <-> GPU`. Each engine owns tile-sized allocations and exposes an async transfer API. No direct DISK↔GPU path.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Transfer granularity | Per tile (256×256 baseline) |
| Ownership | Engine owns; shared via `Arc`; drop triggers flush |
| Async model | Futures / events — every transfer is awaitable |
| Capacity | Engine negotiates with OS / Vulkan budget; overflow triggers cascaded downtransfer |
| Eviction | LRU with pinning (viewport tiles pinned on GPU) |
| Eviction direction | GPU → CPU, CPU → DISK swap, never the reverse on eviction |
| Batching | Transfers batch work units by priority, submit in descending priority |
| GPU layout | One `VkBuffer` per tile (storage buffer, SSBO) |
| DISK decomposition | `FileStorage` (read-only source) + `SwapStorage` (scratch) |

## Tile size

**256×256 pixels**. At RGBA `f16` interleaved, each tile is 512 KiB.

Rationale:
- Supported on every relevant GPU (well under any dimension limit)
- Small enough that neighborhood overhead stays acceptable
- Large enough that per-tile dispatch overhead is amortized
- Reasonable count even for big images — an 8K image is ~510–1024 tiles

Budget sanity check for 8192×8192 RGBA `f16` = 1024 tiles × 512 KiB = 512 MiB. Modern GPUs (8–24 GiB VRAM) hold the whole image plus MIP pyramid comfortably.

## Engine capabilities

| Engine | Storage | Compute | Notes |
|---|---|---|---|
| DISK | yes | no | Source files + swap area |
| CPU | yes | yes | RAM-backed, CPU kernels |
| GPU | yes | yes | VRAM-backed (SSBO), compute shaders |

Compute always happens where the data already lives. Data movement is explicit and scheduled.

## Common engine interface

Each engine exposes roughly:

- `allocate(tile_id) -> Handle` — reserve space
- `read(handle, range) -> Future<Bytes>` — pull data
- `write(handle, data) -> Future<()>` — push data
- `transfer_to(handle, target_engine) -> Future<Handle>` — move between engines
- `evict(handle) -> Future<()>` — proactive release (respects pinning)
- `pin(handle) / unpin(handle)` — mark un-evictable
- `budget() -> (used, total)` — current capacity state

Every operation returns a future. Completion signals propagate via events.

## Ownership and handles

- The engine owns the backing allocation
- Consumers receive an `Arc<Handle>` (reference-counted)
- When the last `Arc` drops, the engine flushes and releases

Flushing on drop means:
- GPU handle drop → if dirty, download to CPU (unless marked transient)
- CPU handle drop → if dirty and part of committed state, write to swap
- DISK handle drop → no-op

Transience: if a tile is an intermediate that no one has held a persistent reference to, it can be discarded without flush. This requires a `transient: bool` flag on the handle (set by the graph executor for non-fork intermediates).

## Pinning

A tile can be **pinned** on an engine. Pinned tiles are excluded from LRU eviction.

Primary use case: **viewport tiles on GPU**. The user is panning/zooming — evicting those tiles would cause visible stalls. The scheduler pins viewport tiles and unpins them when they leave the viewport.

Pinning is per-engine: a tile can be pinned on GPU (viewport) and unpinned on CPU (freely evictable to disk).

## Capacity and overflow

Each engine tracks `(used, total)`:

- **GPU**: Vulkan `VK_EXT_memory_budget` query — honor the driver's advertised soft limit
- **CPU**: platform memory API (sysinfo / `proc/meminfo` on Linux); leave headroom for OS
- **DISK swap**: configurable cap (default: some fraction of free disk space, e.g. 20 GiB or 10% whichever smaller)

Overflow triggers **cascaded downtransfer**:

```
GPU full  → evict to CPU  → if CPU full → evict to DISK swap
CPU full  → evict to DISK swap
DISK swap full → job fails (no further tier)
```

Eviction target is chosen by **LRU among non-pinned tiles** on the engine.

Eviction is always toward the slower tier. An evicted tile is not discarded unless it was marked transient (see Ownership).

## Transfer paths in detail

### DISK ↔ CPU

- Load: decode container (PNG/JPEG/TIFF/…) → raw pixels → color space convert → pack as RGBA `f16` interleaved → place in CPU engine
- Save: inverse (for final output files, not swap)
- Swap paging: tiles written/read as raw `f16` interleaved, optionally LZ4-compressed

### CPU ↔ GPU

- Upload: `VkBuffer` allocation on GPU + staging buffer + `vkCmdCopyBuffer` in a transfer queue
- Download: reverse
- Async: returns a future that completes when the fence signals

### DISK ↔ GPU

- **Not supported directly**. Must go through CPU. See [DECISIONS D1](DECISIONS.md#d1--strict-three-tier-storage-disk---cpu---gpu).

## Async model

Every transfer and every compute dispatch returns a future. Completion events fire via:

- Vulkan fences for GPU work (polled or waited on a dedicated thread)
- OS async I/O (`io_uring` on Linux, IOCP on Windows, `kqueue` on macOS) for DISK
- Rust futures / channels for CPU work

A thin unification layer exposes a single `TransferFuture` type to consumers. Internally, the engine dispatches to the correct backend.

## Batching

Work units and their transfers are **sorted by priority** (see [SCHEDULER](SCHEDULER.md) once written) and submitted in descending order:

- Fill GPU up to budget with highest-priority work units
- Batch compatible transfers into a single Vulkan submit (one `vkQueueSubmit` with multiple command buffers)
- Offload spillover to CPU
- Background prefetch runs at the lowest priority, using leftover bandwidth

## GPU buffer layout

**One `VkBuffer` per tile**, usage flags: `STORAGE_BUFFER | TRANSFER_SRC | TRANSFER_DST`.

Rationale:
- Total tile count is manageable (~1024 for 8K)
- Descriptor set updates become trivial (one binding per tile)
- Neighborhood ops bind central + N neighbors as separate descriptor bindings in one set
- Avoids the complexity of suballocation bookkeeping (VMA-style)

If profiling later shows descriptor-update overhead is significant, suballocate with VMA and treat `(VkBuffer, offset, size)` as the handle. Not doing that up front.

## DISK engine decomposition

The DISK engine is split into two sub-engines with different roles:

### `FileStorage` — source / output files

- **Read-only** for imported source images (never mutate originals)
- Memory-mapped lazy decode: only fetch tiles actually needed
- Output files written once at save time
- Container-aware (PNG, JPEG, TIFF, EXR, …) — driven by file-format layer

### `SwapStorage` — working scratch area

- Read/write paging area for evicted tiles
- Raw `f16` RGBA interleaved, optionally LZ4-compressed
- Tile address in swap stored in the handle
- Space reclaimed when tile returns to a faster engine or when its handle drops
- Compression policy: cold/old tiles compressed, recent tiles left raw (watch CPU cost of compress/decompress vs disk bandwidth)

After a job finishes producing a new committed state, the state may be **flushed** from swap to a compressed form (either LZ4 raw or a proper output file if saving).

## Fallback on transfer failure

**Decided principle** (from [DECISIONS D5](DECISIONS.md#d5--whole-job-failure-on-tile-failure)): a failed **compute** fails the job, but a failed **transfer** falls back to the other engine.

**Open question** — exact protocol. Proposal:

1. Transfer future resolves to `Err(TransferError)`
2. The work unit's planned engine is flipped (e.g. GPU → CPU)
3. The scheduler replans the work unit with the alternate kernel (requires the op to declare a CPU kernel per [D4](DECISIONS.md#d4--operation-capabilities-are-opt-in-per-engine))
4. If the op has no alternate kernel → job fails
5. A bounded retry count (e.g. 2) prevents infinite loops on persistent OOM / device lost

Failure classes considered:
- GPU allocation OOM → retry on CPU
- GPU device lost → full Vulkan restart; job fails this round, retry once after recovery
- CPU allocation OOM → swap to DISK then retry; if DISK swap also full, job fails
- DISK write failure → job fails (no further tier)

**To resolve**: who drives retry? The scheduler (has work-unit context) vs the transfer primitive (close to the error) vs the op runner (knows alternate kernel exists). Leaning scheduler. Confirm.

## Relations

- [DATA_MODEL](DATA_MODEL.md) — what lives in the tile buffers
- [TILE_SYSTEM](TILE_SYSTEM.md) — how tiles are organized per image _(TBD)_
- [SCHEDULER](SCHEDULER.md) — who decides what lives where _(TBD)_
- [EXECUTION_MODEL](EXECUTION_MODEL.md) — how transfers are planned alongside compute _(TBD)_
