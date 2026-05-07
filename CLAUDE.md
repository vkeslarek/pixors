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
├── pixors-engine/         # Rust library + CLI
│   └── src/              # color, image, pixel, convert, io, stream, server, storage, composite
├── pixors-desktop/        # Native desktop app (tao + wry)
│   └── src/              # main.rs, embedded_ui.rs, bridge.js
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

## Architecture

### Engine (pixors-engine)
- **Working space**: ACEScg linear, premultiplied alpha, `f16` storage
- **Color management**: Hardcoded color spaces (sRGB, Rec.709, Rec.2020, ACEScg, etc.)
- **Server**: axum WebSocket on `127.0.0.1:8399` (configurable via `Config { port }`)
- **Config**: `Config { port: u16 }` with `#[derive(Parser)]` + `Default` — no YAML
- **start_server(cfg)** / **start_server_bg(cfg)** — blocking and background variants

### Desktop (pixors-desktop)
- **Framework**: tao 0.35 + wry 0.55
- **Frontend**: embedded via custom protocol `pixors://` (rust-embed from `pixors-ui/dist/`)
- **Dev mode**: `PIXORS_DEV=1` switches to `http://localhost:5173` (Vite)
- **Engine integration**: calls `start_server_bg(Config::default())` on startup
- **Window**: no decorations (custom titlebar), 5px hit-test for native resize

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
| `pixors-executor/src/gpu/scheduler.rs` | Lock-free GPU scheduler (rotating encoder) |
| `pixors-executor/src/runtime/pipeline.rs` | Pipeline compilation + chain runner |
| `pixors-executor/src/stage/` | Producer, Processor, Consumer, Stage traits |
| `pixors-desktop/src/file_ops.rs` | Pipeline graph construction |
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
