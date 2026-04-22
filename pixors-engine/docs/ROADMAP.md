# Roadmap

Phased implementation plan. Order reflects dependency, not calendar.

## Status

- **Design phase**. No engine code written. `src/` is a scaffold; `Cargo.toml` pulls Vulkan/winit but nothing is wired.
- The original `IMPLEMENTATION_PLAN.md` at repo root marked Phase 1 as done. This is incorrect — reset.

## Phase 0 — Design docs (current)

- [x] OVERVIEW
- [x] DATA_MODEL
- [x] DECISIONS
- [x] STORAGE_ENGINES
- [x] TILE_SYSTEM
- [x] MIP_PYRAMID
- [x] OPERATION_GRAPH
- [x] OPERATIONS
- [x] EXECUTION_MODEL
- [x] SCHEDULER
- [ ] EDITOR_SEMANTICS
- [ ] API

## Phase 1 — Foundation

- Storage engine trait with CPU implementation (RAM-backed)
- Tile data structure (`f16` RGBA interleaved, 256×256 baseline)
- Basic color space conversion (at least sRGB ↔ ACEScg)
- Simple image I/O (PNG load, PNG save) with CS conversion at the boundary
- Smoke test: load → identity pipeline → save, round-trip diff within quantization tolerance

## Phase 2 — Operation graph (CPU only)

- `ValueId`, `OperationGraph`, `OperationNode`
- Graph construction API on a `Context`
- Lazy execution — `ctx.run()` materializes
- Simple per-pixel ops (brightness, contrast, invert, premul/unpremul)
- Neighborhood-aware ops (Gaussian blur, box blur)
- Work unit formation for tile + neighbor groups

## Phase 3 — GPU backend

- Vulkan setup (instance, device, queues, allocator)
- GPU storage engine (SSBO-backed tile storage)
- Compute shader compilation pipeline
- GPU kernels for existing CPU ops
- CPU↔GPU transfer scheduling
- Fallback to CPU on GPU transfer failure

## Phase 4 — Scheduler and priority

- Job queue with priority levels (viewport-interactive down to prefetch)
- Resource budgets per engine
- Adaptive offload decisions (CPU vs GPU)
- Viewport tracking hook
- Tile cache with eviction

## Phase 5 — MIP pyramid

- Pyramid data structure per image
- MIP generation ops (at least box + gamma-correct bilinear)
- Viewport-aware MIP selection
- Invalidation on edits

## Phase 6 — Editor semantics

- Layers
- Masks, selections
- Non-destructive history / undo
- Blend modes

## Phase 7 — File formats

- JPEG, TIFF, EXR, WebP
- ICC profile parsing (hardcoded fast path + `lcms2` fallback)
- 8u / 16u / 16f / 32f in/out

## Phase 8 — Advanced ops and integrations

- Geometric ops (resize, rotate, warp, crop)
- Color ops (curves, levels, color balance)
- Compositing ops (Porter–Duff, blend modes)

## Phase 9 — Frontends

- Rust API stabilization
- Python bindings
- CLI
- Standalone GUI application
- MCP server

## Out of current scope

- Vector graphics
- Video
- 3D / raytracing
- Plugin system
- Tile-level fault tolerance

## Notes

- Each phase ends with measurable tests. A phase is not done until there is a round-trip test exercising the new surface.
- Performance targets (latency, throughput) are deferred to after Phase 3 — premature without a working GPU backend.
