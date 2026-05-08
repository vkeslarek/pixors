# Pixors – AI Assistant Context

## Project Overview

Pixors is an open-source image editor — Rust engine + React frontend, shipped as a single desktop binary.

## Repository Structure

```
.
├── Cargo.toml             # Workspace root (shared version, edition, lints)
├── Makefile               # Build tasks
├── CONTRIBUTING.md        # Coding guidelines
├── scripts/               # build-linux.sh, build-windows.sh, build-macos.sh
├── .github/workflows/     # CI (main) + Release (release/*)
├── pixors-executor/         # Rust library + CLI
│   └── src/              # color, image, pixel, data, data_transform, operation, gpu, sink, source, stage, runtime
├── pixors-desktop/        # Native desktop app (tao + wry)
│   └── src/              # main.rs, app.rs, controller.rs, state/, action/, viewport/, components/, pages/
└── pixors-ui/             # React + TypeScript + Vite frontend
```

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
- **`context.device` is authoritative.** The pipeline compiler (`assign_devices`) sets it; processors only read it. A processor receiving a `Buffer::Gpu` tile when `context.device == Cpu` is a runtime bug, not something the processor should paper over.
- **`assign_devices` uses a heuristic to minimise transfers.** `StageHints { device, preference }` on every stage. Fixed `Cpu`/`Gpu` nodes are assigned first. `Either` nodes are assigned iteratively: preference match → all‑same‑adjacent → GPU default. This groups stages into maximal same‑device chains before `insert_transfers` adds `Upload`/`Download` bridges.
- **`NeighborhoodData` is dual‑device.** `Cpu { tiles: Vec<Tile> }` stores pointer‑accumulated tiles for CPU blur (assemble padded buffer on CPU, one upload). `Gpu { consolidated: Arc<GpuBuffer>, tile_infos: Vec<TileGpuInfo> }` stores a single contiguous GPU buffer built by `TileToNeighborhood`'s GPU path via `copy_buffer_to_buffer`, so blur can assemble its padded buffer entirely on the device.
- **`Scheduler::download_buffer` does not exist.** Batch GPU→CPU download is done exclusively by `DownloadProcessor` via staging buffers. Individual GPU‑buffer reads (e.g. for debugging or r=0 passthrough) use `Scheduler::read_from_buffer`, which allocates staging, copies, maps, and returns `Vec<u8>` in a single call.

## Architecture

### Engine (pixors-executor)
- **Working space**: ACEScg linear, `f16` storage (configurable via `EditorState.working_format`/`working_color_space`)
- **Display space**: sRGB `Rgba8` (configurable via `EditorState.display_format`/`display_color_space`)
- **Color management**: Hardcoded color spaces (sRGB, Rec.709, Rec.2020, ACEScg, etc.)
- **Pipeline**: DAG of stages (Source → DataTransform → Operation → Sink), compiled by `Pipeline::compile()`, executed via `ChainRunner` threads
- **GPU**: wgpu compute, SPIR‑V shaders via slang, `Scheduler` owns encoder rotation + buffer pool + pipeline cache
- **Cancellation**: `Pipeline::run()` returns `PipelineHandle` with `.cancel()` (sets `AtomicBool` on `ChainRunner`, checked between tiles)

### Desktop (pixors-desktop)
- **Framework**: tao 0.35 + wry 0.55
- **Frontend**: embedded via custom protocol `pixors://` (rust-embed from `pixors-ui/dist/`)
- **Dev mode**: `PIXORS_DEV=1` switches to `http://localhost:5173` (Vite)
- **Engine integration**: calls `start_server_bg(Config::default())` on startup
- **Window**: no decorations (custom titlebar), 5px hit-test for native resize
- **State**: `EditorState` owns tabs, pipeline lock, working/display format+color space
- **Actions**: `prepare → apply → undo` pattern, `Dispatcher` with per‑tab routing

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
| `pixors-executor/src/common/color/space.rs` | ColorSpace enum + primaries/transfer/whitepoint |
| `pixors-executor/src/common/color/conversion.rs` | ColorConversion engine, `convert_pixels`, `convert_bytes` |
| `pixors-executor/src/common/color/model.rs` | ColorModelTransform enum (CMYK→RGB, YCbCr→RGB) |
| `pixors-executor/src/common/pixel/format.rs` | PixelFormat enum + model_transform mapping |
| `pixors-executor/src/common/pixel/mod.rs` | AlphaPolicy, Pixel trait, re-exports |
| `pixors-executor/src/common/pixel/{rgba,rgb,gray,cmyk,ycbcr}.rs` | Pixel trait impls per model |
| `pixors-executor/src/common/image/mod.rs` | Image, ImageDescriptor, PageInfo, Metadata |
| `pixors-executor/src/common/image/exif.rs` | Metadata enum + EXIF parsers |
| `pixors-executor/src/common/image/codec.rs` | ImageDecoder + PageStream traits |
| `pixors-executor/src/common/image/{png,tiff}/` | PNG/TIFF codec implementations |
| `pixors-executor/src/operation/color.rs` | ColorConvert pipeline stage (CPU + GPU) |
| `pixors-executor/shaders/color.slang` | GPU color convert entry points |
| `pixors-executor/shaders/lib/color.slang` | Shader library: transfer, codecs, color_convert |
| `pixors-executor/src/gpu/scheduler.rs` | Lock-free GPU scheduler (rotating encoder, buffer pool, copy/dispatch API) |
| `pixors-executor/src/runtime/pipeline.rs` | Pipeline compilation + chain runner + `assign_devices` heuristic |
| `pixors-executor/src/stage/` | Producer, Processor, Consumer, Stage, StageHints traits |
| `pixors-executor/src/data/neighborhood.rs` | Neighborhood + dual‑device NeighborhoodData (Cpu/Gpu) |
| `pixors-executor/src/data_transform/to_neighborhood.rs` | TileToNeighborhood stage (CPU pointer accumulation + GPU buffer consolidation) |
| `pixors-executor/src/operation/blur.rs` | Box blur stage (CPU + GPU paths, zero‑download GPU assembly) |
| `pixors-desktop/src/state/` | EditorState, Tab, History multi‑tab editor model |
| `pixors-desktop/src/action/` | Action trait + Dispatcher + per‑action pipeline orchestration |
| `pixors-desktop/src/main.rs` | App entry point, tracing config |

## How to add a new PixelFormat

1. **`common/pixel/format.rs`** — add variant (e.g. `CmykA8`), update `channel_count`, `sample_bytes`, `model_transform`
2. **`common/pixel/{model}.rs`** — create/update pixel struct (e.g. `CmykA<T>`), add `unsafe impl Pod/Zeroable`, impl `Pixel` for `u8`/`u16`/`f16`/`f32`. `unpack()` must return `[f32;4]` in `[0,1]` range.
3. **`common/pixel/mod.rs`** — add `pub use`
4. **`common/color/model.rs`** — if non-RGB model (CMYK, YCbCr, Lab), add `ColorModelTransform` variant + `decode_4`/`decode_1` SIMD logic. Must have `#[repr(u32)]` with discriminants matching the shader.
5. **`shaders/lib/color.slang`** — add matching variant to `ColorModel` enum + branch in `color_convert()`
6. **`common/color/conversion.rs`** — add `(src_fmt, dst_fmt)` match arms in `convert_bytes()`
7. **`operation/color.rs`** — update `precision()`, `bytes_per_pixel()`, `channels()` for GPU dispatch
8. **`common/image/tiff/stream.rs`** / **`common/image/png/mod.rs`** — map from format-specific color type to new PixelFormat
9. **Tests** — add `unpack`/`pack`/`convert_pixels` tests in conversion.rs

## How to add a new ColorSpace

1. **`common/color/primaries.rs`** — add `RgbPrimaries` variant with xy chromaticity coordinates
2. **`common/color/transfer.rs`** — add `TransferFn` variant with `decode()`/`encode()` functions
3. **`common/color/space.rs`** — add `ColorSpace` variant or static constructor with primaries + whitepoint + transfer
4. **`common/color/matrix.rs`** — ensure the new primaries can compute a 3×3 matrix to/from XYZ
5. **`shaders/lib/color.slang`** — add `TransferFn` variant + `decode_tf`/`encode_tf` branches
6. **`common/color/detect.rs`** — update ICC classifier to recognize the new space
7. **`operation/color.rs`** — update `tf_u32()` mapping
