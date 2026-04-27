# Pixors

**Open Source image editor — Rust engine + React frontend + Native desktop shell**

> ⚠️ **Not production ready. Not even testing ready yet.** This entire project is under active development. APIs, architecture, and features change frequently. Use at your own risk.

## Architecture

```
pixors/
├── pixors-engine/     # Image processing engine (Rust library + WebSocket server)
├── pixors-desktop/    # Native desktop shell (tao + wry, borderless window)
├── pixors-ui/         # React + TypeScript frontend (Vite)
└── docs/              # Architecture docs, phase plans, known bugs
```

### `pixors-engine` — The Engine
Rust library that does all heavy lifting. Runs as a WebSocket server (axum). Communicates with the frontend via a binary protocol (MessagePack commands, raw tile frames).

**Key subsystems:**
- **Stream pipeline** — Tiles flow through pipes (source → color convert → MIP → sinks) in dedicated threads with bounded channels
- **Color science** — ACEScg linear f16 working space, LUT-based conversion, SIMD 4-wide
- **Tile storage** — RAM cache (ViewportSink) + disk persistence (WorkingSink, ACEScg f16)
- **MIP pyramid** — Recursive 2×2 box-filter, generated eagerly in the stream pipeline
- **Compositor** — Porter-Duff over blend, per-tile, stateless
- **I/O** — PNG & TIFF readers via `ImageReader` trait

### `pixors-ui` — The Frontend
React + TypeScript + Vite. Custom panel docking system (no external library). Zustand state management persisted to localStorage.

### `pixors-desktop` — The Desktop Shell
Borderless native window via **tao** + **wry**. IPC bridge (JS→Rust) for window controls. Cross-platform: Linux (GTK/WebKit), Windows (WebView2), macOS (WKWebView).

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
| WebSocket server (tab/viewport/session services) | ✅ |
| Custom panel docking (drag, resize, persist) | ✅ |
| Borderless desktop window (tao + wry) | ✅ |
| Window controls (min/max/close/drag/resize) | ✅ |
| Cross-compile Windows support | ✅ |

### Planned / In Progress

| Feature | Status |
|---|---|
| Operations (blur, contrast, brightness) | 🚧 Phase 9 |
| Job system with progress tracking | 🚧 Phase 9 |
| Preview (MIP-aware, per-zoom-level) | 🚧 Phase 9 |
| Component-per-service backend refactor | 🚧 Phase 9 |
| Library workspace (browse & organize) | 📋 |
| Darkroom workspace (develop & adjust) | 📋 |
| Error surface (typed errors, toasts) | 📋 Phase 9 |
| Desktop shell distribution (single binary) | 📋 |
| MCP integration (LLM-driven editing) | 📋 |
| Layer adjustments (non-destructive) | 📋 |
| Selection tools (marquee, lasso, wand) | 📋 |
| Export (PNG, TIFF, JPEG, WebP) | 📋 |
| macOS testing & support | 📋 |

## Getting Started

### Prerequisites
- Rust (latest stable)
- Node.js 18+
- Linux: `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`

### Development

```bash
git clone https://github.com/vkeslarek/pixors.git
cd pixors

# Engine server
cd pixors-engine && cargo run

# Frontend (separate terminal)
cd pixors-ui && npm install && npm run dev

# Desktop shell
cd pixors-desktop && cargo run
```

### Testing

```bash
cd pixors-engine && cargo test --lib
```

## Documentation

| Doc | Description |
|---|---|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Full architecture reference |
| [PHASE_9.md](docs/PHASE_9.md) | Current phase plan |
| [KNOWN_BUGS.md](docs/KNOWN_BUGS.md) | Known issues |
| [ROADMAP.md](docs/ROADMAP.md) | Future ideas |

## License

MIT
