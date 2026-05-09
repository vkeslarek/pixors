# Pixors

**Open-source image editor — Rust engine + native desktop (iced)**

> ⚠️ **Not production ready.** Active development — APIs, architecture, and features change frequently.

## Architecture

```
pixors/
├── pixors-engine/     # Framework: Stage trait, data types, GPU infra, runtime
├── pixors-shader/     # GPU shaders (Slang → SPIR-V) + compiled binaries
├── pixors-color/      # Color science: ColorConvert, ColorConversion, pixel models
├── pixors-image/      # Image I/O: codec traits, PNG/TIFF, sources, encoder sinks
├── pixors-ops/        # Operations: Blur, Compose, MipDownsample, MipFilter
├── pixors-state/      # Headless editor state: tabs, layers, actions, dispatcher
├── pixors-desktop/    # Native desktop app (iced 0.14, wgpu, borderless window)
└── docs/              # Architecture docs, phase plans, known bugs
```

### Crate Dependency Graph

```
pixors-engine  ←  pixors-color  ←  pixors-image  ←  pixors-ops
     ↑                ↑                                   ↑
pixors-shader  ───────┘                         pixors-state
                                                     ↑
                                             pixors-desktop
```

### `pixors-engine` — The Framework
Defines all traits (`Stage`, `Producer`, `Processor`, `Consumer`, `Pixel`, `GpuKernel`) and provides the execution runtime.

- **Stage system** — `Stage` trait with dynamic dispatch, `Arc<dyn Stage>` in pipeline graphs
- **Pipeline** — DAG compiled by `Pipeline::compile()`, executed via `ChainRunner` threads
- **GPU** — wgpu compute, SPIR-V shaders, lock-free `Scheduler` (encoder rotation + buffer pool + pipeline cache)
- **Data types** — `Tile`, `ScanLine`, `TileBlock`, `Neighborhood` flow through bounded channels

### `pixors-shader` — GPU Shaders
Slang source + `lib/` modules + compiled SPIR-V binaries checked into `kernels/`.
`build.rs` recompiles when `slangc` is available; falls back to committed SPV otherwise.

### `pixors-color` — Color Science
`ColorConvert` stage (CPU + GPU, SIMD via `wide`), `ColorConversion` engine, pixel model structs (`Rgba`, `Rgb`, `Gray`, `Cmyk`, `YCbCr`, `Lab`).

### `pixors-image` — Image I/O
PNG & TIFF decoders/encoders, `ImageStreamSource`, encoder sinks, disk tile cache (`CacheWriter`).

### `pixors-ops` — Operations
`Blur` (CPU + GPU box blur), `Compose` (Porter-Duff over), `MipDownsample` (recursive 2×2), `MipFilter`.

### `pixors-state` — Headless State
`EditorState`, `Tab`, `Action` trait, `Dispatcher` — no GUI dependencies. Drives the desktop and MCP server alike.

### `pixors-desktop` — Native App
Iced 0.14 desktop shell. Viewport rendered via wgpu into an iced custom widget. Per-tab GPU texture atlas, camera/pan/zoom, blur preview pipeline.

## Features

### Implemented

| Feature | Status |
|---|---|
| PNG decode with color space detection | ✅ |
| TIFF decode (single & multi-page) | ✅ |
| PNG + TIFF encode with full config | ✅ |
| Color space conversion (sRGB, Rec.709, P3, ACEScg, …) | ✅ |
| ACEScg f16 linear working space | ✅ |
| Parallel tile pipeline (producer → processor → consumer) | ✅ |
| MIP pyramid generation (recursive 2×2) | ✅ |
| Tile compositor (Porter-Duff alpha-over) | ✅ |
| Box blur (CPU + GPU via wgpu compute) | ✅ |
| Live blur preview (overlay tile cache) | ✅ |
| Viewport: pan, zoom, MIP-aware tile fetch | ✅ |
| Headless state layer (no GUI deps) | ✅ |
| Export modal (PNG + TIFF, full config) | ✅ |
| Windows cross-compile | ✅ |

### Roadmap

| Phase | Goal | Status |
|---|---|---|
| 9 · Engine foundation | Action/Dispatcher, headless state, GPU buffer safety, viewport moved to desktop | ✅ Done |
| **10 · First complete loop** | Export fix, layer UX (select/visibility/opacity), per-layer blur + preview, composite display, JPEG + WebP, controller routing | 🚧 In progress |
| 11 · Formats + blend modes + Library | AVIF, multi-layer TIFF, blend modes (Multiply, Screen, Overlay…), Library workspace v1 | 📋 |
| 12 · RAW v1 | Canon CR3, demosaicing, white balance, sensor→ACEScg color matrix | 📋 |
| 13 · RAW v2 | NEF, ARW, DNG, camera profiles, HEIC/HEIF | 📋 |
| 14 · Darkroom | Non-destructive op pipeline: tonal + color ops, Tone Curve, HSL | 📋 |
| 15 · Masking | SAM integration, geometric tools, brush tools, matting | 📋 |
| 16 · Selection engine | Quick select, magic wand, color range, luminance mask | 📋 |
| 17 · Layer Editor ops | Sharpen/USM, crop, rotate, flip, vignette, grain | 📋 |

Full detail: [ROADMAP.md](docs/ROADMAP.md)

## Getting Started

### Prerequisites

- Rust (latest stable)
- Linux: `libgtk-3-dev libxkbcommon-dev libwayland-dev libx11-dev libfontconfig1-dev`
- macOS / Windows: no extra deps

Optional (shader recompilation only):
- [Slang compiler](https://github.com/shader-slang/slang/releases) — only needed when modifying `.slang` files; pre-compiled SPIR-V is checked in

### Development

```bash
git clone https://github.com/vkeslarek/pixors.git
cd pixors

# Run the desktop app
cargo run -p pixors-desktop

# Full workspace check + lint
cargo check --workspace
cargo clippy --workspace
```

### Testing

```bash
cargo test --workspace
```

## Documentation

| Doc | Description |
|---|---|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Full architecture reference |
| [ROADMAP.md](docs/ROADMAP.md) | Phase plan and backlog |
| [PHASE_10.md](docs/PHASE_10.md) | Current phase — detailed implementation spec |
| [KNOWN_BUGS.md](docs/KNOWN_BUGS.md) | Known issues |

## License

MIT
