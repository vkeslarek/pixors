# Pixors

**Open Source image editor — Rust engine + React frontend + Native desktop shell**

> ⚠️ **Not production ready. Not even testing ready yet.** This entire project is under active development. APIs, architecture, and features change frequently. Use at your own risk.

## Architecture

```
pixors/
├── pixors-engine/     # Framework: Stage trait, data types, GPU infra, runtime
├── pixors-shader/     # All GPU shaders + compiled SPIR-V binaries
├── pixors-color/      # Color science: ColorConvert, ColorConversion, pixel models
├── pixors-image/      # Image I/O: codec traits, PNG/TIFF, image sources, encoder sinks
├── pixors-ops/        # Operations: Blur, Compose, MipDownsample, MipFilter
├── pixors-state/      # Editor state model: tabs, layers, actions, history, viewport state
├── pixors-desktop/    # Native desktop app (iced, borderless window)
├── pixors-ui/         # React + TypeScript frontend (Vite)
└── docs/              # Architecture docs, phase plans, known bugs
```

### Crate Dependency Graph

```
pixors-engine  ←  pixors-color  ←  pixors-image  ←  pixors-ops
     ↑                ↑                              ↑
pixors-shader  ───────┘                     pixors-state
     ↑                                          ↑
pixors-desktop  ─── pixors-image, pixors-ops, pixors-state
```

### `pixors-engine` — The Framework
Rust library that defines all traits (`Stage`, `Producer`, `Processor`, `Consumer`, `Pixel`, `GpuKernel`) and provides the execution runtime. No internal dependencies.

**Key subsystems:**
- **Stage system** — `Stage` trait with dynamic dispatch, `Arc<dyn Stage>` in pipeline graphs
- **Pipeline** — DAG of stages compiled by `Pipeline::compile()`, executed via `ChainRunner` threads
- **GPU** — wgpu compute, SPIR‑V shaders, `Scheduler` owns encoder rotation + buffer pool + pipeline cache
- **Data types** — `Tile`, `ScanLine`, `TileBlock`, `Neighborhood` flow through bounded channels
- **Color types** — `ColorSpace`, `TransferFn`, `PixelFormat`, `AlphaPolicy`

### `pixors-shader` — GPU Shaders
All `.slang` shader source + shared `lib/` modules + compiled SPIR-V binaries exported as `pub const`.

### `pixors-color` — Color Science
- `ColorConvert` stage (CPU + GPU, SIMD via `wide`)
- `ColorConversion` engine (LUT‑based, matrix transforms)
- Pixel model structs: `Rgba<T>`, `Rgb<T>`, `Gray<T>`, `Cmyk<T>`, `YCbCr<T>`, `Lab<T>`

### `pixors-image` — Image I/O
- `Image` struct, `ImageDescriptor`, `Dpi`, codec traits
- PNG & TIFF decoders/encoders
- `ImageStreamSource`, encoder sinks (`PngEncoderV2`, `TiffEncoderStage`), `CacheWriter`

### `pixors-ops` — Operations
- `Blur` — box blur (CPU + GPU)
- `Compose` — layer compositing (Porter-Duff over blend)
- `MipDownsample` — recursive 2×2 box-filter
- `MipFilter` — pass‑through filter by mip level

### `pixors-desktop` — The Desktop Shell
Borderless native window via **iced 0.14**. Actions pattern (`prepare → apply → undo`). Viewport stages for GPU rendering.

### `pixors-ui` — The Frontend
React + TypeScript + Vite. Custom panel docking system. Zustand state management.

## Features

### Implemented

| Feature | Status |
|---|---|
| PNG loading with color space detection | ✅ |
| TIFF loading (single & multi-page) | ✅ |
| Color space conversion (sRGB, Rec.709, P3, ACEScg, etc.) | ✅ |
| ACEScg f16 linear working space | ✅ |
| Stream pipeline (parallel tile processing) | ✅ |
| MIP pyramid generation (recursive 2×2) | ✅ |
| Tile compositor (Porter-Duff over blend) | ✅ |
| Box blur (CPU + GPU) | ✅ |
| Custom panel docking (drag, resize, persist) | ✅ |
| Borderless desktop window (iced) | ✅ |
| Cross-compile Windows support | ✅ |

### Roadmap

| Phase | Goal | Status |
|---|---|---|
| 9 · Engine foundation | Action/Dispatcher system, headless state layer, GPU buffer safety, viewport split desktop↔state | ✅ Done |
| **10 · First complete loop** | Export fix, layer UX (select/visibility/opacity), per-layer blur filter + live preview, composite display pipeline, JPEG + WebP decode/encode, controller routing refactor | 🚧 In progress |
| 11 · Formats + blend modes + Library | JPEG, WebP, AVIF decode; multi-layer TIFF; blend modes (Multiply, Screen, Overlay…); Library workspace v1 (thumbnails, ratings, EXIF) | 📋 |
| 12 · RAW v1 | Canon CR3 decode, demosaicing, white balance, color matrix sensor→ACEScg | 📋 |
| 13 · RAW v2 | NEF, ARW, DNG, camera profiles, HEIC/HEIF | 📋 |
| 14 · Darkroom | Non-destructive op pipeline: tonal + color ops, Tone Curve, HSL, Color Grading | 📋 |
| 15 · Masking | SAM integration, geometric selection tools, brush tools, matting | 📋 |
| 16 · Selection engine | Quick select, magic wand, color range, luminance mask, Quick Mask mode | 📋 |
| 17 · Layer Editor ops | Sharpen/USM, crop, rotate, flip, vignette, grain, color grading wheels | 📋 |

Full detail: [ROADMAP.md](docs/ROADMAP.md)

## Getting Started

### Prerequisites
- Rust (latest stable)
- Node.js 18+
- Linux: `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`

### Development

```bash
git clone https://github.com/vkeslarek/pixors.git
cd pixors

# Desktop app (main entry point)
cargo run -p pixors-desktop

# Frontend (separate terminal, for dev mode)
cd pixors-ui && npm install && npm run dev

# Dev mode (desktop connects to Vite instead of embedded frontend)
PIXORS_DEV=1 cargo run -p pixors-desktop
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
