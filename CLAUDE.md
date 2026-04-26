# Pixors – Claude Context

This file provides context about the Pixors project for Claude/assistant.

## Project Overview

Pixors is an open‑source image editor written in Rust with a React frontend. The architecture follows a phased implementation plan where each phase delivers a concrete, runnable feature.

**Current status**: Phase 2 (Viewport, Swapchain, Interactivity) is complete. The codebase has been refactored into separate crates: engine, viewport (WASM experiment), and UI frontend.

## Repository Structure

```
.
├── pixors-engine/          # Main Rust library + CLI
│   ├── src/               # Engine source (color, image, pixel, convert, io, viewport)
│   ├── docs/              # Design documents (OVERVIEW, DECISIONS, PHASE_2, etc.)
│   └── Cargo.toml
├── pixors-viewport/       # WebAssembly bindings (experimental)
│   └── src/lib.rs        # WASM entry point
├── pixors-ui/            # React + TypeScript frontend
│   ├── src/              # React components
│   ├── public/           # Static assets
│   └── package.json
├── .gitignore            # Root gitignore (Rust + Node)
├── README.md             # Main project README
└── CLAUDE.md             # This file
```

## Key Architectural Decisions

### Engine (pixors-engine)

- **Working space**: ACEScg linear, premultiplied alpha, `f16` storage
- **Color management**: Hardcoded color spaces (sRGB, Rec.709, Rec.2020, ACEScg, etc.)
- **Image representation**: `TypedImage<P: Pixel>` with compile‑time pixel type
- **I/O**: PNG only for Phase 1, with proper color space detection
- **Viewport system**: Phase 2 introduced `ImageView`, `ViewRect`, `Swapchain`, `Viewport`
- **Rendering**: Nearest‑neighbor + bicubic (Catmull‑Rom) sampling, ready for SIMD optimization
- **Interactivity**: Mouse pan & zoom with perfect anchor preservation

### Frontend (pixors-ui)

- **Framework**: React + TypeScript + Vite
- **Build**: Modern toolchain with ESLint
- **Future integration**: Will communicate with engine via WebAssembly (pixors-viewport)

### Development Philosophy

- **Phased delivery**: Each phase must produce a runnable demo
- **No early optimization**: Keep implementation simple until profiling shows need
- **API‑first**: Programmatic Rust API is primary deliverable; UI/CLI/MCP come later
- **Design docs as reference**: `docs/` describes end‑state; `IMPLEMENTATION_PLAN.md` is the execution path

## Phase Summary

### Phase 1 – Image I/O Abstraction (✓)
- PNG load/save with correct color management
- Conversion pipeline: any source → ACEScg f16 premul
- Round‑trip tests

### Phase 2 – Viewport & Interactivity (✓)
- **ImageView**: Non‑owning reference to ARGB u32 data
- **ViewRect**: Camera with pan(dx, dy) and zoom(factor, anchor)
- **Swapchain**: Circular buffer pool (N configurable, default 2)
- **Viewport**: Orchestrator with dirty‑flag rendering
- **Sampling**: Nearest‑neighbor (floor) + bicubic scalar
- **Integration**: Winit‑based desktop viewer for testing (not the main UI)

### Phase 3 – Operations Basics (▶️ Next)
- Synchronous CPU‑only operations
- `Operation` trait with `apply(&self, input: &Image, params) -> Result<Image>`
- Ops: brightness, contrast, gamma, invert, gain, color‑matrix, premul/unpremul
- CLI: `pixors‑cli apply --brightness 1.2 input.png output.png`

### Phase 4 – Async Engine per Tile (⏳ Future)
- Tiled representation (256×256)
- Async execution on thread pool
- Neighborhood ops (blur, etc.)
- MIP pyramid generation

### Phase 5 – UI (⏳ Future)
- React frontend with viewport widget
- Op controls, histogram, layer stack
- Real‑time feedback with cancellation

### Phase 6 – Editor Semantics (⏳ Future)
- Layers, masks, selections
- Non‑destructive adjustment layers
- Undo/redo history

## Important Notes

- **Winit is for testing only**: The desktop viewer (`pixors-engine/src/main.rs`) uses `winit` + `softbuffer` solely to validate Phase 2 interactivity. The production UI is the web frontend (`pixors-ui`).
- **Viewport communication**: The `pixors-viewport` crate is an experiment in compiling viewport code to WebAssembly for browser rendering. This is not yet integrated with the UI.
- **Color format**: The viewport currently expects ARGB u32 pixels (sRGB u8). This is a pragmatic choice for Phase 2; future phases may sample directly from ACEScg f16.
- **SIMD ready**: Bicubic sampling is implemented in scalar form but structured to accept SIMD optimization via the `wide` crate (already a dependency).

## Development Commands

```bash
# Build engine
cd pixors-engine
cargo build
cargo test

# Run desktop viewer (test)
cargo run -- example1.png

# Build & run frontend
cd ../pixors-ui
npm install
npm run dev

# Check all Rust crates
cargo check --workspace
```

## Design Documents

Refer to `docs/` for authoritative design decisions:

- `OVERVIEW.md` – Principles & scope
- `DATA_MODEL.md` – Pixel format, color, alpha
- `DECISIONS.md` – Locked architectural choices (D1–D55+)
- `PHASE_2.md` – Viewport & swapchain specification
- `IMPLEMENTATION_PLAN.md` – Pragmatic phased plan

## Recent Changes

- **2026‑04‑21**: Phase 2 implementation completed. Code refactored into separate crates. All tests pass (78 unit tests). Created root `.gitignore`, `README.md`, and this `CLAUDE.md`.

When working on Pixors, always check the current phase in `IMPLEMENTATION_PLAN.md` and respect the “no early optimization” principle. Keep changes focused on delivering the current phase’s runnable outcome.