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
| `pixors-engine/src/config.rs` | `Config { port: u16 }` |
| `pixors-engine/src/server/server.rs` | `start_server(cfg)`, `start_server_bg(cfg)` |
| `pixors-engine/src/main.rs` | CLI entry point |
| `pixors-desktop/src/main.rs` | Tao event loop, custom protocol, IPC |
| `pixors-desktop/src/embedded_ui.rs` | rust-embed for `pixors-ui/dist/` |
| `pixors-desktop/src/bridge.js` | JS ↔ Rust IPC bridge |
| `pixors-desktop/build.rs` | Copies WebView2Loader.dll on Windows |
| `pixors-ui/src/engine/client.ts` | WebSocket client, hardcoded port 8399 |
| `CONTRIBUTING.md` | Coding guidelines for humans |
