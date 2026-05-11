# Pixors Architecture

> Current as of May 2026 — Phase 10: Transform model + GPU Compose.
> See `CLAUDE.md` and `AGENTS.md` for agent quick reference.

---

## 1. Big Picture

Pixors is an open-source image editor with a **pipeline-based GPU/CPU processing engine**
(`pixors-engine`) and an **Iced desktop GUI** (`pixors-desktop`). Image data flows as
**tiles** (256×256) through a graph of stages compiled into parallel chains.

```
File → ImageStreamSource → ScanLineToTile → ColorConvert → MipDownsample
                                                              ├─→ CacheWriter (disk)
                                                              └─→ ColorConvert → ViewportSink (GPU texture)
```

---

## 2. Crate Map

```
pixors/
├── pixors-engine/     # Runtime: Pipeline, ChainRunner, graph, GPU scheduler, data types
├── pixors-shader/     # Slang GPU shaders compiled to SPIR-V
├── pixors-color/      # Color space conversion, pixel types
├── pixors-image/      # Image codecs (PNG, TIFF), Image stream, CacheWriter
├── pixors-ops/        # Operations: Blur, Compose, MipDownsample, MipFilter, CacheReader
├── pixors-state/      # Editor model: EditorState, actions, Dispatcher, TileCache, Camera
├── pixors-desktop/    # Iced GUI: viewport, widgets, panels, dialogs
└── pixors-mcp/        # TypeScript MCP server (calls pixors-state headlessly)
```

Dependency order (no cycles):
```
pixors-engine
    ↑
pixors-shader → pixors-color
                    ↑
                pixors-image
                    ↑
                pixors-ops
                    ↑
                pixors-state
                    ↑
        pixors-desktop   pixors-mcp
```

---

## 3. Core Concepts

### Tile-based processing

All image data flows as 256×256 tiles. No full-image buffer exists after decode.
Tiles carry a `TileCoord` (mip_level, tx, ty, px, py, width, height) and pixel data
in `Buffer` (CPU `Vec<u8>` or GPU `Arc<GpuBuffer>`).

### Pipeline

`Pipeline::compile()` takes an `ExecGraph` (DAG of `Arc<dyn Stage>`) and:
1. Assigns devices (CPU/GPU) to each stage
2. Inserts Upload/Download transfer stages between device boundaries
3. Detects chains (consecutive same-device stages)
4. Builds inter-chain channels (bounded `sync_channel`)
5. Creates `ChainRunner` per chain

`pipeline.run()` spawns one thread per chain. Each chain runs stages sequentially,
producing/processing/consuming items.

### Actions

Every state mutation is an `Action`. `Dispatcher::dispatch()` calls `prepare()`,
compiles and runs the pipeline, and calls `apply()` on completion. Two modes:
- `Background` — non-blocking, cancellable (previews, fetches)
- `Apply` — modal, locks the tab (export, destructive ops)

### TileCache

Two-tier in-memory tile buffer: `base` (gen=0, never evicted) and `overlay`
(gen>0, preview pipelines). The viewport renders overlay over base.

### Document Model

A `Document` holds a flat list of `LayerNode`s. Each `LayerNode` owns:
- `source: PixelSource` — `PrimaryAsset { page }` or `SolidColor`
- `blend: BlendSpec { mode, opacity }`
- `transforms: Vec<Transform>` — ordered list of operations applied to the layer

Each `Transform` has:
- `op: Operation` — `Blur { radius }`, `Exposure { stops }` (Exposure: todo)
- `input: InputScope` — `Layer` (self), `Below` (composite below), `Reference(NodeId)`
- `output: OutputMode` — `Replace { blend }` or `Composite { blend, position }`

`compile(doc, req, config, sink) -> ExecGraph` is the pure function that turns a
`Document` into a runnable graph. It lives in `pixors-document/src/render/compiler.rs`.
The desktop's `run_mip_fetch` calls it directly, passing `TileCacheSink` as the sink.

---

## 4. Pipeline Flow (Open File Example)

```
OpenFile action
  │
  ├─ prepare() → open_image(), build ExecGraph, return PreparedAction
  │
  ├─ dispatch() → Pipeline::compile()
  │     │
  │     ├─ Chain #0 [Cpu] ImageStream → ScanLineToTile → Upload
  │     ├─ Chain #1 [Gpu] ColorConvert → MipDownsample
  │     ├─ Chain #2 [Cpu] Download → CacheWriter
  │     ├─ Chain #3 [Gpu] ColorConvert (to display)
  │     └─ Chain #4 [Cpu] Download → ViewportCacheSink
  │
  └─ apply(Done) → push_tab(), tiles now visible in viewport
```

---

## 5. Key Files

| File | Purpose |
|------|---------|
| `pixors-engine/src/runtime/pipeline.rs` | `Pipeline::compile()` + `run()`, device assignment |
| `pixors-engine/src/runtime/chain.rs` | `ChainRunner`, progress reporting |
| `pixors-engine/src/gpu/scheduler.rs` | GPU dispatch, buffer pool, readback |
| `pixors-engine/src/stage/node.rs` | `Stage` trait, `StageHints` |
| `pixors-state/src/action/mod.rs` | `Action` trait, `Dispatcher`, `PreparedAction` |
| `pixors-state/src/state/tab.rs` | `Tab`, `TabId` |
| `pixors-state/src/state/editor.rs` | `EditorState` |
| `pixors-desktop/src/controller.rs` | `App::update()` — message routing |
| `pixors-desktop/src/viewport/pipeline.rs` | `ViewportPrimitive` — GPU texture upload |
| `pixors-desktop/src/viewport/program.rs` | `ViewportProgram` — camera, pan, zoom |

---

## 6. GPU Integration

The engine manages its own wgpu instance separate from Iced's wgpu. The scheduler
batches dispatch calls into encoder slots, flushing when full or at download points.

Buffer pool uses deferred recycling: dropped buffers go to a `pending` lock-free
queue and are only recycled at safe points (`flush()` or `Device::poll(Wait)`).

SPIR-V shaders are precompiled from Slang in `pixors-shader` via the `#[kernel]`
proc-macro in `pixors-shader-macro`. Each kernel generates a `*ParamsKernel` type
(e.g. `BlurParamsKernel`, `ComposeParamsKernel`) that implements `GpuKernel`.

`Compose` runs on GPU when `assign_devices` places it there: N layers trigger N
sequential pairwise alpha-over dispatches, starting from a zeroed transparent
accumulator (opacity per layer applied via `opacity_b` parameter).

---

## 7. Viewport Rendering

The `ViewportProgram` (Iced shader widget) reads the camera state (`Arc<RwLock<Camera>>`)
to compute the visible MIP level and tile range. A fullscreen triangle shader samples
the GPU texture atlas with camera pan/zoom/mip. Tiles are uploaded incrementally via
`TiledTexture::write_tile_cpu()`.
