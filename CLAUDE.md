# Pixors – AI Assistant Context

## Project Overview

Pixors is an open-source image editor — Rust engine + React frontend, shipped as a single desktop binary.

## Repository Structure

```
.
├── Cargo.toml                 # Workspace root (shared version, edition, lints)
├── Makefile                   # Build tasks
├── CONTRIBUTING.md            # Coding guidelines
├── AGENTS.md                  # Compact guide for AI agents
├── scripts/                   # build-linux.sh, build-windows.sh, build-macos.sh
├── .github/workflows/         # CI (main) + Release (release/*)
├── pixors-engine/             # Framework: Stage/Pipeline traits, data types, GPU infra, runtime
│   └── src/                  # stage, data, data_transform, graph, gpu, runtime, operation/transfer, error, utils, common/{color,pixel}
├── pixors-shader/             # All GPU shaders + compiled SPIR-V binaries
│   └── {shaders/, kernels/, src/lib.rs}
├── pixors-color/              # Color science: ColorConvert stage, ColorConversion engine, pixel model structs
│   └── src/                  # operation/color, common/{color/conversion, pixel/{rgba,rgb,gray,cmyk,ycbcr,lab}}
├── pixors-image/              # Image I/O: codec traits, Image, PNG/TIFF codecs, image sources, encoder sinks
│   └── src/                  # common/image, source/image_stream, sink/{png_encoder*,tiff_encoder,cache_writer}
├── pixors-ops/                # Operations: Blur, Compose, MipDownsample, MipFilter, CacheReader
│   └── src/                  # operation/{blur,compose,mip_*}, source/cache_reader
├── pixors-document/              # Headless application state: EditorState, tabs, actions, dispatcher, tile cache
│   └── src/                  # state/{editor,tab,history,viewport_cache,camera}, action/{mod,dispatcher,actions/*}, viewport_cache_{source,sink}.rs
├── pixors-desktop/            # Desktop GUI (Iced): renders state, no business logic
│   └── src/                  # main.rs, app.rs, controller.rs, components/, pages/, widgets/, dialog/, viewport/, icons.rs, theme.rs
│                             # viewport/{pipeline,program,sink,tiled_texture}.rs (GPU atlas + screen render)
├── pixors-mcp/                # MCP server (TypeScript/Node): drives pixors-document headlessly over stdio
│   └── src/                  # MCP tool handlers → dispatch Actions against EditorState
└── pixors-ui/                 # React + TypeScript + Vite frontend (future web UI)
```

## Crate Dependency Graph

```
pixors-engine  ←  pixors-color  ←  pixors-image  ←  pixors-ops
     ↑                ↑                                   ↑
pixors-shader  ───────┘                                   │
                                                          │
pixors-document  ──────────────── pixors-engine, pixors-color, pixors-image, pixors-ops
     ↑
pixors-desktop  ─── pixors-document  (+ direct deps on engine/color/image/ops for viewport stages)
pixors-mcp      ─── pixors-document  (headless, no GUI)
```

- **`pixors-engine`** — No internal deps. Defines all traits (`Stage`, `Producer`, `Processor`, `Consumer`, `GpuKernel`, `Runner`, `Pixel`, `Component`, `ImageDecoder`, `PageStream`, `ImageEncoder`) and supporting types (`Device`, `Buffer`, `Tile`, `ScanLine`, `TileBlock`, `Neighborhood`, `PixelFormat`, `ColorSpace`, `TransferFn`, `PixelMeta`, `AlphaPolicy`, `DataKind`, `PortSpecification`, `StageHints`, `ProcessorContext`, `Item`, `ExecGraph`, `Pipeline`, `ChainRunner`, `Scheduler`, `GpuContext`, `Upload`, `Download`, `DataTransformNode` variants).
- **`pixors-shader`** — No deps. Owns all `.slang` files + `build.rs` (slangc → SPIR-V) + compiled SPV exports (`COLOR_SPV`, `BLUR_SPV`, `MIP_DOWNSAMPLE_SPV`).
- **`pixors-color`** — Depends on `pixors-engine`, `pixors-shader`. `ColorConvert` stage (CPU+GPU), `ColorConversion` engine, pixel model structs (`Rgba<T>`, `Rgb<T>`, `Gray<T>`, `Cmyk<T>`, `YCbCr<T>`, `Lab<T>`).
- **`pixors-image`** — Depends on `pixors-engine`, `pixors-color`. `Image` struct, `ImageDescriptor`, `PageInfo`, `Dpi`, codec traits, PNG/TIFF codecs, `ImageStreamSource`, `PngEncoder`, `PngEncoderV2`, `TiffEncoderStage`, `CacheWriter`.
- **`pixors-ops`** — Depends on `pixors-engine`, `pixors-color`, `pixors-image`, `pixors-shader`. `Blur`, `Compose`, `MipDownsample`, `MipFilter`, `CacheReader`.
- **`pixors-document`** — Depends on `pixors-engine`, `pixors-color`, `pixors-image`, `pixors-ops`. `EditorState`, `Tab`, `ViewportCache`, `Camera`, `Action` trait, `Dispatcher`, concrete actions (`OpenFile`, `BlurPreview`, `Export`, …), `ViewportCacheSource`/`ViewportCacheSink` pipeline stages. **No GUI deps (no iced, no wgpu, no rfd).** Designed to be driven headlessly by MCP or CLI.
- **`pixors-desktop`** — Depends on `pixors-document` + direct deps on engine/color/image/ops for viewport-specific stages. Iced `App` struct, all UI components and widgets, `ViewportSink` (GPU→screen stage), `TiledTexture` (GPU atlas), wgpu render pipeline. Pure view layer — contains zero business logic.
- **`pixors-mcp`** — TypeScript/Node MCP server. Calls into `pixors-document` (via FFI or subprocess) to dispatch `Action`s without a window.

## Code Style

- **cargo fmt** before commit — non‑negotiable
- **cargo clippy --workspace** before push — lint levels in workspace `Cargo.toml` (`[workspace.lints.clippy]`)
- **Well thought abstractions** make the code easy to read, too many abstractions make it unreadable
- **Follow existing patterns**: look at neighboring files for naming, structure, idioms
- **Conventional commits**: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`

## Branch Strategy

- `main` — latest development state
- `feature/*` — feature branches, merge into `main` via PR
- `release/X.Y.Z` — triggers CI build + GitHub release for all platforms

## Development Commands

```bash
# Workspace
cargo check --workspace
cargo test --workspace
cargo clippy --workspace
cargo fmt --all

# Build targets
make build-front       # npm build in pixors-ui
make build             # build engine + desktop (release)
make release-linux     # frontend + engine + desktop (Linux)
make release-windows   # cross-compile for Windows
make check             # cargo check --workspace
make test              # cargo test --workspace

# Dev mode (connects to Vite dev server instead of embedded frontend)
PIXORS_DEV=1 cargo run -p pixors-desktop

# Frontend
cd pixors-ui && npm run dev
```

## Pipeline Invariants

These are non-negotiable. Violations are bugs, not shortcuts.

- **Processors never move pipeline data between devices.** No `upload_bytes`, no `read_from_buffer` in a `Processor`/`Producer`/`Consumer` impl for pipeline data (tiles, neighborhoods). CPU↔GPU transitions are the runtime's job: `assign_devices` picks where each stage runs, `insert_transfers` injects `Upload`/`Download` stages at device boundaries automatically. A processor that runs on GPU will always receive GPU buffers; one that runs on CPU will always receive CPU buffers — trust `context.device`.
  - Exception: creating internal scratch buffers (e.g. a zeroed padded buffer for blur) via `scheduler.alloc_zeroed_buffer()` or `scheduler.upload_bytes()` is fine — this is not moving pipeline data, it's allocating working memory.
- **Processors never reference `wgpu` directly.** All GPU interaction (buffer allocation, copies, dispatches, reads) goes through `Scheduler`. A Processor that calls `wgpu::Device`, `wgpu::Queue`, or `wgpu::CommandEncoder` directly is a layering violation. The `Scheduler` owns encoder rotation, batch flushing, buffer pool, and pipeline cache — Processors use its high-level API only. GPU context comes from `ctx.gpu.as_ref()`, never from `gpu::context::try_init()`.
- **`context.device` is authoritative.** The pipeline compiler (`assign_devices`) sets it; processors only read it. A processor receiving a `Buffer::Gpu` tile when `context.device == Cpu` is a runtime bug, not something the processor should paper over.
- **`assign_devices` uses a heuristic to minimise transfers.** `StageHints { device, preference }` on every stage. Fixed `Cpu`/`Gpu` nodes are assigned first. `Either` nodes are assigned iteratively: preference match → all‑same‑adjacent → GPU default. This groups stages into maximal same‑device chains before `insert_transfers` adds `Upload`/`Download` bridges.
- **`NeighborhoodData` is dual‑device.** `Cpu { tiles: Vec<Tile> }` stores pointer‑accumulated tiles for CPU blur (assemble padded buffer on CPU, one upload). `Gpu { consolidated: Arc<GpuBuffer>, tile_infos: Vec<TileGpuInfo> }` stores a single contiguous GPU buffer built by `TileToNeighborhood`'s GPU path via `copy_buffer_to_buffer`, so blur can assemble its padded buffer entirely on the device.
- **`Scheduler::download_buffer` does not exist.** Batch GPU→CPU download is done exclusively by `DownloadProcessor` via staging buffers. Individual GPU‑buffer reads (e.g. for debugging or r=0 passthrough) use `Scheduler::read_from_buffer`, which allocates staging, copies, maps, and returns `Vec<u8>` in a single call.

## Architecture

### pixors-engine — The Framework
- **Stage system**: `Stage` trait (`Send + Sync + Debug`) with dynamic dispatch via `Arc<dyn Stage>`. Each stage provides `kind()`, `ports()`, `hints()`, `producer()`, `processor()`, `consumer()`.
- **Pipeline data types**: `Tile`, `ScanLine`, `TileBlock`, `Neighborhood` flow through bounded channels between stages.
- **Data transforms**: `ScanLineToTile`, `TileToScanline`, `TileToTileBlock`, `TileToNeighborhood` — infrastructure adapters between data formats.
- **GPU subsystem**: `GpuContext` (singleton wgpu device), `Scheduler` (lock‑free encoder rotation, buffer pool, dispatch/copy API), `PipelineCache` (compute pipeline cache). Stages interact with GPU exclusively through `Scheduler`.
- **Runtime**: `Pipeline::compile()` — DAG compilation (port validation, device assignment, transfer insertion, chain detection). `ChainRunner` — threaded execution of producer→kernels→consumer chains. `PipelineHandle` — cancellation via `AtomicBool`.
- **Working space**: ACEScg linear, `f16` storage (configurable via `EditorState.working_format`/`working_color_space`)
- **Display space**: sRGB `Rgba8` (configurable via `EditorState.display_format`/`display_color_space`)

### pixors-shader — GPU Shaders
- All `.slang` shader source + `lib/` modules (pixel, neighborhood, convolution, transfer, params, codecs, convert).
- `build.rs` compiles via `slangc` to SPIR-V in `kernels/`.
- `src/lib.rs` exports SPV binaries as `pub const COLOR_SPV`, `BLUR_SPV`, `MIP_DOWNSAMPLE_SPV`.

### pixors-color — Color Science
- `ColorConvert` stage (CPU + GPU paths, SIMD via `wide`).
- `ColorConversion` engine (LUT‑based, matrix transforms).
- Pixel model structs: `Rgba<T>`, `Rgb<T>`, `Gray<T>`, `GrayAlpha<T>`, `Cmyk<T>`, `CmykA<T>`, `YCbCr<T>`, `Lab<T>` — implement `Pixel` trait from engine.

### pixors-image — Image I/O
- `Image` struct, `ImageDescriptor`, `PageInfo`, `Dpi`, codec traits.
- PNG/TIFF decoders (`PngDecoder`, `TiffDecoder`) and encoders (`PngImageEncoder`, `TiffImageEncoder`).
- `ImageStreamSource` (produces ScanLine from image), encoder sinks (`PngEncoderV2`, `TiffEncoderStage`), `CacheWriter` (disk LZ4 cache).

### pixors-ops — Operations
- `Blur` — box blur (CPU + GPU, Neighborhood→Tile).
- `Compose` — layer compositing (CPU, variable Tile inputs).
- `MipDownsample` — recursive 2×2 (CPU + GPU, pass‑through + TileBlock).
- `MipFilter` — pass‑through filter by mip level.
- `CacheReader` — reads tiles from disk LZ4 cache.

### pixors-document — Headless Application State

**Why it exists**: MCP server and future CLI/automation need to open files, run pipelines, and mutate state without a window. `pixors-document` is the model layer — it knows nothing about Iced widgets, wgpu textures, GPU atlases, file dialogs, or rendering. Any consumer that can hold an `EditorState` and call `Dispatcher::dispatch()` is a valid frontend.

- **`EditorState`** — owns `Vec<Tab>`, `active: Option<TabId>`, pipeline lock, working/display format+color space.
- **`Tab`** — one open image: `ImageDescriptor`, `ViewportCache` (in-memory tile buffer), `Camera` (tile range / MIP math), layers, undo history, pipeline signals.
- **`ViewportCache`** — two-tier in-memory tile cache: `base` (gen=0, source-of-truth, never overwritten by previews) + `overlay` (gen>0, preview results, dropped on cancel). Naming note: called "viewport" because the desktop reads it to render, but the cache itself has no display knowledge.
- **`Camera`** — pure math: given zoom/pan and image dimensions, computes which MIP level to use and which `TileRange` to request. No wgpu, no rendering.
- **`Action` trait** — `prepare(&mut EditorState) → PreparedAction`, `apply(…)`, `undo(…)`. All state mutations go through actions.
- **`Dispatcher`** — action lifecycle: validate → prepare → spawn pipeline thread or apply immediately → route `PipelineEvent` back to caller. Per-tab locking prevents concurrent pipelines on the same tab.
- **Concrete actions** (`action/actions/`): `OpenFile`, `BlurPreview`, `BlurCancel`, `Export`, `RequestMipFetch`, `SwitchTab`, `CloseTab`.
- **`ViewportCacheSource` / `ViewportCacheSink`** — pipeline `Stage` impls for reading/writing the in-memory tile cache. Live in state (not desktop) because MCP pipelines also need tile I/O without a screen.

**Do NOT add to pixors-document**: Iced types, wgpu handles, rfd dialogs, GPU texture atlases, window/event loop state.

### pixors-desktop — Desktop GUI

Pure view layer. No business logic. Renders `EditorState` via Iced, manages GPU texture atlas.

- **Framework**: iced 0.14
- **`App` struct**: holds `Dispatcher` + `EditorState` (both from `pixors-document`) plus all UI component states (tab bar, toolbar, panels, dialogs).
- **`Msg` enum**: all UI events — `Action(Arc<dyn Action>)`, `PipelineEvent`, component-specific messages.
- **Update loop** (`controller.rs`): routes `Msg` → calls `dispatcher.dispatch()`, updates component state, triggers redraws.
- **Viewport GPU** (`viewport/`): `ViewportSink` (GPU→screen stage), `TiledTexture` (wgpu texture atlas), `ViewportPipeline` (wgpu render pipeline, tiled fragment shader, `CameraUniform`).
- **Components** (`components/`): `tab_bar`, `toolbar`, `filters_panel`, `layers_panel`, `status_bar`, `menu_bar`, `workspace_bar`, `viewport`.
- **Dialogs** (`dialog/`): `ExportDialog` (format, bit depth, compression, DPI, ICC options).
- **Widgets** (`widgets/`): Iced extensions (`LoadingBar`, `Tooltip`, `Pill`, …).

**Do NOT add to pixors-desktop**: `EditorState` mutations, pipeline construction, action business logic, tile cache management. All of those belong in `pixors-document`.

**UI Guidelines**: See [UI.md](UI.md) for detailed rules on component standardisation, Modals vs Dialogs, and UX architecture.

### State ↔ Desktop boundary rules

| Lives in `pixors-document` | Lives in `pixors-desktop` |
|---|---|
| `EditorState`, `Tab`, `ViewportCache` | `App` struct, `Msg` enum |
| `Action` trait + concrete actions | Iced widgets and component state |
| `Dispatcher` | wgpu `TiledTexture`, `ViewportPipeline` |
| `Camera` (tile range math) | Camera → `CameraUniform` GPU upload |
| `ViewportCacheSource/Sink` (stages) | `ViewportSink` (screen-render stage) |
| Pipeline graph construction (`PathBuilder`) | File dialogs (`rfd`) |
| `PipelineEvent` enum | Progress bar / toast UI |

### Frontend (pixors-ui)
- **Framework**: React + TypeScript + Vite
- **WebSocket**: connects to `ws://127.0.0.1:8399/ws` (hardcoded in `src/engine/client.ts`)

### CI/CD
- **CI** (`.github/workflows/ci.yml`): check → test → clippy on `main` and PRs
- **Release** (`.github/workflows/release.yml`): builds Linux/Windows/macOS on `release/*` branches, creates GitHub release

## Clippy Lint Levels (workspace Cargo.toml)

**Deny** (breaks build): `collapsible_if`, `doc_overindented_list_items`, `excessive_precision`, `io_other_error`, `manual_div_ceil`, `manual_is_multiple_of`, `needless_borrow`, `needless_range_loop`, `ptr_arg`, `redundant_closure`, `slow_vector_initialization`, `unnecessary_cast`, `unnecessary_map_or`

**Warn**: `question_mark`

**Allow**: `module_inception`, `new_without_default`, `too_many_arguments`, `type_complexity`

## Key Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Workspace config, shared version, lint levels |
| `pixors-engine/src/stage/node.rs` | `Stage` trait, `StageHints`, `StageRole` |
| `pixors-engine/src/stage/actors.rs` | `Producer`, `Processor`, `Consumer` traits |
| `pixors-engine/src/stage/context.rs` | `ProcessorContext` |
| `pixors-engine/src/stage/kinds.rs` | `DataKind`, `PortSpecification` |
| `pixors-engine/src/data/tile.rs` | `Tile`, `TileCoord`, `TileGridPos` |
| `pixors-engine/src/data/neighborhood.rs` | `Neighborhood` + dual‑device `NeighborhoodData` |
| `pixors-engine/src/data/buffer.rs` | `Buffer` enum (Cpu/Gpu) |
| `pixors-engine/src/graph/graph.rs` | `ExecGraph<Arc<dyn Stage>>`, `EdgePorts`, `StageId` |
| `pixors-engine/src/graph/item.rs` | `Item` enum (Tile/ScanLine/Neighborhood/TileBlock) |
| `pixors-engine/src/gpu/scheduler.rs` | Lock-free `Scheduler` (encoder rotation, buffer pool, dispatch/copy) |
| `pixors-engine/src/gpu/context.rs` | `GpuContext` singleton |
| `pixors-engine/src/gpu/kernel.rs` | `GpuKernel` trait, `KernelSignature` |
| `pixors-engine/src/runtime/pipeline.rs` | `Pipeline::compile()`, `Pipeline::run()`, `assign_devices`, `insert_transfers` |
| `pixors-engine/src/runtime/chain.rs` | `ChainRunner` |
| `pixors-engine/src/operation/transfer/upload.rs` | `Upload` stage (CPU→GPU, auto‑injected) |
| `pixors-engine/src/operation/transfer/download.rs` | `Download` stage (GPU→CPU, auto‑injected) |
| `pixors-engine/src/data_transform/to_neighborhood.rs` | `TileToNeighborhood` (CPU pointer accumulation + GPU consolidation) |
| `pixors-engine/src/common/color/space.rs` | `ColorSpace` enum + primaries/transfer/whitepoint |
| `pixors-engine/src/common/color/transfer.rs` | `TransferFn` enum + decode/encode |
| `pixors-engine/src/common/color/primaries.rs` | `RgbPrimaries` enum, `WhitePoint` |
| `pixors-engine/src/common/color/matrix.rs` | `Matrix3x3` color space transforms |
| `pixors-engine/src/common/color/model.rs` | `ColorModelTransform` enum (CMYK→RGB, YCbCr→RGB, Lab→RGB) |
| `pixors-engine/src/common/pixel/mod.rs` | `Pixel` trait, `Component` trait, `AlphaPolicy` |
| `pixors-engine/src/common/pixel/format.rs` | `PixelFormat` enum (30+ variants) |
| `pixors-engine/src/common/pixel/meta.rs` | `PixelMeta` (format + colorspace + alpha) |
| `pixors-shader/src/lib.rs` | SPV exports: `COLOR_SPV`, `BLUR_SPV`, `MIP_DOWNSAMPLE_SPV` |
| `pixors-shader/shaders/lib/transfer.slang` | `TransferFn` enum + `decode_tf`/`encode_tf` |
| `pixors-shader/shaders/lib/params.slang` | `ChannelLayout`, `AlphaPolicy`, `ColorModel`, `ColorConvertParams` |
| `pixors-shader/shaders/lib/codecs.slang` | `IPixelCodec` interface + U8/U16/F16/F32 codecs |
| `pixors-shader/shaders/lib/convert.slang` | `color_convert()` + `cc_kernel` template |
| `pixors-shader/shaders/lib/pixel.slang` | `rgba8_unpack`/`rgba8_pack`, pixel helpers |
| `pixors-shader/shaders/lib/neighborhood.slang` | Neighborhood data structures |
| `pixors-shader/shaders/lib/convolution.slang` | Convolution/blur helpers |
| `pixors-shader/shaders/color.slang` | Color convert compute shader entry points |
| `pixors-shader/shaders/blur.slang` | Box blur compute shader entry points |
| `pixors-shader/shaders/mip_downsample.slang` | MIP downsample compute shader entry points |
| `pixors-color/src/operation/color.rs` | `ColorConvert` stage (CPU + GPU) |
| `pixors-color/src/common/color/conversion.rs` | `ColorConversion` engine, `convert_pixels`, `convert_bytes` |
| `pixors-color/src/common/pixel/rgba.rs` | `Rgba<T>` struct + `impl Pixel` |
| `pixors-color/src/common/pixel/rgb.rs` | `Rgb<T>` struct + `impl Pixel` |
| `pixors-color/src/common/pixel/gray.rs` | `Gray<T>`, `GrayAlpha<T>` + `impl Pixel` |
| `pixors-color/src/common/pixel/cmyk.rs` | `Cmyk<T>`, `CmykA<T>` + `impl Pixel` |
| `pixors-color/src/common/pixel/ycbcr.rs` | `YCbCr<T>` + `impl Pixel` |
| `pixors-color/src/common/pixel/lab.rs` | `Lab<T>` + `impl Pixel` |
| `pixors-image/src/common/image/mod.rs` | `Image`, `ImageDescriptor`, `PageInfo`, `Dpi`, `open_image()` |
| `pixors-image/src/common/image/codec.rs` | `ImageDecoder`, `PageStream`, `ImageEncoder` traits |
| `pixors-image/src/common/image/exif.rs` | `Metadata` enum + EXIF parsers |
| `pixors-image/src/common/image/png/` | PNG codec (`PngDecoder`, `PngPageStream`, `PngImageEncoder`) |
| `pixors-image/src/common/image/tiff/` | TIFF codec (`TiffDecoder`, `TiffPageStream`, `TiffImageEncoder`) |
| `pixors-image/src/source/image_stream.rs` | `ImageStreamSource` (emits ScanLine from Image) |
| `pixors-image/src/sink/png_encoder_v2.rs` | `PngEncoderV2` (Tile→PNG) |
| `pixors-image/src/sink/tiff_encoder.rs` | `TiffEncoderStage` (Tile→TIFF) |
| `pixors-image/src/sink/cache_writer.rs` | `CacheWriter` (Tile→disk LZ4 cache) |
| `pixors-ops/src/operation/blur.rs` | `Blur` stage (CPU + GPU box blur) |
| `pixors-ops/src/operation/compose.rs` | `Compose` stage (layer compositing) |
| `pixors-ops/src/operation/mip_downsample.rs` | `MipDownsample` stage (recursive 2×2) |
| `pixors-ops/src/operation/mip_filter.rs` | `MipFilter` stage (pass‑through filter) |
| `pixors-ops/src/source/cache_reader.rs` | `CacheReader` (reads tiles from disk LZ4 cache) |
| `pixors-document/src/state/editor.rs` | `EditorState` — owns all tabs, pipeline lock, working/display format |
| `pixors-document/src/state/tab.rs` | `Tab` — one open image: descriptor, cache, camera, layers, history |
| `pixors-document/src/state/viewport_cache.rs` | `ViewportCache` — two-tier (base/overlay) in-memory tile cache |
| `pixors-document/src/state/camera.rs` | `Camera` — tile range math (MIP level, visible tiles, zoom/pan) |
| `pixors-document/src/state/history.rs` | `History` — undo/redo snapshot stack |
| `pixors-document/src/action/mod.rs` | `Action` trait, `PreparedAction`, `PipelineMode` |
| `pixors-document/src/action/dispatcher.rs` | `Dispatcher` — action lifecycle, pipeline dispatch, per-tab locking |
| `pixors-document/src/action/actions/` | Concrete actions: `OpenFile`, `BlurPreview`, `BlurCancel`, `Export`, `RequestMipFetch`, `SwitchTab`, `CloseTab` |
| `pixors-document/src/viewport_cache_sink.rs` | `ViewportCacheSink` — pipeline stage: writes tiles to in-memory cache |
| `pixors-document/src/viewport_cache_source.rs` | `ViewportCacheSource` — pipeline stage: reads tiles from in-memory cache |
| `pixors-document/src/path_builder.rs` | `PathBuilder` — constructs `ExecGraph` from `Arc<dyn Stage>` |
| `pixors-desktop/src/main.rs` | App entry point, tracing config |
| `pixors-desktop/src/app.rs` | `App` struct + subscriptions (tick, keyboard, pipeline events) |
| `pixors-desktop/src/controller.rs` | `App::update()` — routes `Msg`, calls dispatcher |
| `pixors-desktop/src/viewport/sink.rs` | `ViewportSink` — GPU→screen stage (wgpu texture copy) |
| `pixors-desktop/src/viewport/tiled_texture.rs` | `TiledTexture` — wgpu GPU texture atlas (one atlas per MIP) |
| `pixors-desktop/src/viewport/pipeline.rs` | `ViewportPipeline` — wgpu render pipeline, tiled fragment shader |
| `pixors-desktop/src/components/viewport.rs` | Iced viewport widget — hosts wgpu surface, handles mouse/scroll |

## How to add a new PixelFormat

1. **`pixors-engine/src/common/pixel/format.rs`** — add variant, update `channel_count`, `sample_bytes`, `model_transform`
2. **`pixors-color/src/common/pixel/{model}.rs`** — create/update pixel struct, add `unsafe impl Pod/Zeroable`, impl `Pixel` for `u8`/`u16`/`f16`/`f32`. `unpack()` must return `[f32;4]` in `[0,1]` range.
3. **`pixors-color/src/common/pixel/mod.rs`** — add `pub use`
4. **`pixors-engine/src/common/color/model.rs`** — if non-RGB model, add `ColorModelTransform` variant + `decode_4`/`decode_1` SIMD logic. Must have `#[repr(u32)]` with discriminants matching the shader.
5. **`pixors-shader/shaders/lib/params.slang`** — add matching variant to `ColorModel` enum
6. **`pixors-shader/shaders/lib/convert.slang`** — add branch in `color_convert()`
7. **`pixors-color/src/common/color/conversion.rs`** — add `(src_fmt, dst_fmt)` match arms in `convert_bytes()`
8. **`pixors-color/src/operation/color.rs`** — update `precision()`, `bytes_per_pixel()`, `channels()` for GPU dispatch
9. **`pixors-image/src/common/image/{png,tiff}/`** — map from format-specific color type to new PixelFormat
10. **Tests** — add `unpack`/`pack`/`convert_pixels` tests

## How to add a new ColorSpace

1. **`pixors-engine/src/common/color/primaries.rs`** — add `RgbPrimaries` variant with xy chromaticity coordinates
2. **`pixors-engine/src/common/color/transfer.rs`** — add `TransferFn` variant with `decode()`/`encode()` functions
3. **`pixors-engine/src/common/color/space.rs`** — add `ColorSpace` variant or static constructor with primaries + whitepoint + transfer
4. **`pixors-engine/src/common/color/matrix.rs`** — ensure the new primaries can compute a 3×3 matrix to/from XYZ
5. **`pixors-shader/shaders/lib/transfer.slang`** — add `TransferFn` variant + `decode_tf`/`encode_tf` branches
6. **`pixors-engine/src/common/color/detect.rs`** — update ICC classifier to recognize the new space
7. **`pixors-color/src/operation/color.rs`** — update `tf_u32()` mapping

## How to add a new Stage

1. Implement `Stage` (and `Producer`/`Processor`/`Consumer` as appropriate) for your struct.
2. Add your stage to the appropriate crate based on its domain:
   - Color‑related → `pixors-color`
   - Image I/O → `pixors-image`
   - Operations → `pixors-ops`
   - Viewport/desktop‑specific → `pixors-desktop`
3. If it needs GPU shaders, add `.slang` files to `pixors-shader/shaders/` and export the SPV in `pixors-shader/src/lib.rs`.
4. Use `Arc::new(YourStage { ... })` in `PathBuilder` chains.
5. Add a public re‑export from your crate's `lib.rs` or module root.
