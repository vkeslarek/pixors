# Pixors

**Image processing engine with web UI** – A modern, high-performance image editor written in Rust with a React frontend.

> **Status**: Active development – Phase 2 (Viewport & Interactivity) complete

## Architecture

Pixors is structured as a multi-crate Rust workspace with a separate web frontend:

```
pixors/
├── pixors-engine/        # Core image processing library (Rust)
│   ├── src/
│   │   ├── color/        # Color science & management
│   │   ├── image/        # Image data structures
│   │   ├── pixel/        # Pixel types & components
│   │   ├── convert/      # Conversion pipelines
│   │   ├── io/          # PNG I/O
│   │   ├── viewport/     # Viewport, swapchain, pan/zoom
│   │   └── lib.rs
│   └── docs/            # Design documents
├── pixors-viewport/      # WebAssembly viewport (experimental)
├── pixors-ui/           # React + TypeScript frontend
└── pixors/              # CLI application (future)
```

### Core Principles

- **GPU-first**: All operations optimized for parallel execution
- **Color-managed**: ACEScg linear working space, proper ICC handling
- **Tile-based**: Gigapixel-scale processing with efficient caching
- **Asynchronous**: Non-blocking API with priority scheduling
- **Cross-platform**: Native desktop + web via WebAssembly

## Current Features

### Phase 1 – Image I/O Abstraction (✓ Complete)
- PNG loading/saving with color space detection
- ACEScg linear f16 internal representation
- Premultiplied alpha workflow
- Color space conversions (sRGB, Rec.709, Rec.2020, ACEScg, etc.)
- Transfer function decoding/encoding

### Phase 2 – Viewport & Interactivity (✓ Complete)
- `ImageView`: Non‑owning reference to ARGB pixel data
- `ViewRect`: Camera with pan and anchor‑preserving zoom
- `Swapchain`: Tear‑free circular buffer pool
- `Viewport`: Orchestrator with dirty‑flag rendering
- Bicubic interpolation (Catmull‑Rom kernel)
- Mouse‑driven navigation (drag to pan, scroll to zoom)

### Phase 3 – Operations (In Progress)
- CPU‑side image operations (brightness, contrast, gamma, etc.)
- Operation trait design
- Pipeline composition

## Getting Started

### Prerequisites

- Rust (latest stable) – for the engine
- Node.js 18+ – for the frontend
- Git

### Development

```bash
# Clone the repository
git clone https://github.com/anomalyco/pixors.git
cd pixors

# Build the engine
cd pixors-engine
cargo build

# Run the desktop viewer (test image)
cargo run -- example1.png

# Run the web frontend
cd ../pixors-ui
npm install
npm run dev
```

### Testing

```bash
# Run all Rust tests
cd pixors-engine
cargo test

# Run specific module tests
cargo test --lib viewport
```

## Design Documents

Detailed architecture decisions are in `pixors-engine/docs/`:

- [OVERVIEW.md](pixors-engine/docs/OVERVIEW.md) – Goals & principles
- [DATA_MODEL.md](pixors-engine/docs/DATA_MODEL.md) – Pixel format, color, alpha
- [DECISIONS.md](pixors-engine/docs/DECISIONS.md) – Locked architectural decisions
- [PHASE_2.md](pixors-engine/docs/PHASE_2.md) – Viewport & swapchain specification

## Roadmap

See [ROADMAP.md](pixors-engine/docs/ROADMAP.md) and [IMPLEMENTATION_PLAN.md](pixors-engine/docs/IMPLEMENTATION_PLAN.md) for detailed phase breakdown.

**Next up**: Phase 3 (Operations basics) – synchronous CPU‑only ops with CLI.

## Contributing

Contributions are welcome! Please read the design documents first to understand the architecture. Focus on one phase at a time.

## License

MIT – See [LICENSE](LICENSE) file.