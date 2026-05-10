# Pixors — Agent Quick Reference

Compact guide for AI agents. Full context in CLAUDE.md.

---

## What is Pixors?

Open-source image editor. Rust workspace + TypeScript MCP server. Pipeline-based GPU/CPU image processing engine with a desktop GUI (Iced) and a headless API (MCP).

---

## Crates at a glance

| Crate | Language | What it owns | What it does NOT own |
|---|---|---|---|
| `pixors-engine` | Rust | `Stage` enum, `Producer`/`Processor`/`Consumer` traits, GPU scheduler, data types (`Tile`, `Buffer`, `Neighborhood`…), runtime, color science types (`ColorSpace`, `TransferFn`, `Matrix3x3`…) | No operations, no image I/O, no app state |
| `pixors-shader` | Slang/Rust | `.slang` GPU shaders + `#[kernel]` proc-macro generated SPV + Rust kernel types | No runtime, no pipeline logic |
| `pixors-shader-macro` | Rust (proc-macro) | `#[kernel]` attribute macro — reads annotation, calls slangc, generates SPV + `GpuKernel` impls | No runtime |
| `pixors-image` | Rust | Image codecs (PNG, TIFF, JPEG), `Image` struct, `CacheWriter`, pixel types (`Rgba<T>`, `Rgb<T>`, `Gray<T>`…) | No color science, no operations |
| `pixors-ops` | Rust | `Blur`, `Compose`, `MipDownsample`, `MipFilter`, `CacheReader`, `ColorConvert` | No app state, no GUI |
| `pixors-state` | Rust | `EditorState`, `Tab`, actions, `Dispatcher`, `PathBuilder` | No GUI widgets, no wgpu textures, no file dialogs |
| `pixors-desktop` | Rust | Iced GUI, wgpu GPU atlas (`TiledTexture`), screen render (`ViewportSink`), dialogs | No business logic, no pipeline construction |
| `pixors-mcp` | TypeScript | MCP server — calls `pixors-state` headlessly over stdio | No GUI |

---

## Dependency order (no cycles allowed)

```
pixors-engine
    ↑
pixors-shader ──→ pixors-shader-macro
    ↑
pixors-image
    ↑
pixors-ops
    ↑
pixors-state
    ↑
pixors-desktop    pixors-mcp
```

If your change would reverse an arrow, stop — you have a design problem.

---

## Where does new code go?

**New pixel format?** → See "How to add a new PixelFormat" in CLAUDE.md (10 steps across engine/color/shader/image).

**New GPU operation (blur-like)?** → `pixors-ops/src/processor/`, shader in `pixors-shader/shaders/`.

**New image codec?** → `pixors-image/src/{png,tiff}/`.

**New editor action (open, export, filter…)?** → `pixors-state/src/action/actions/`. Must implement `Action` trait. No Iced or wgpu imports.

**New UI panel or widget?** → `pixors-desktop/src/components/` or `panel/`. No `EditorState` mutation here — emit a `Msg::Action(…)` instead. See `UI.md` for component guidelines.

**New MCP tool?** → `pixors-mcp/src/`, calls `Dispatcher::dispatch()` on `EditorState`.

**New pipeline stage for tile I/O tied to in-memory cache?** → `pixors-state/src/` (like `ViewportCacheSource/Sink`). Not in desktop — MCP needs these too.

---

## The state/desktop split (most confusing part)

`pixors-state` is the **model**. It has no window, no Iced, no wgpu textures. It can run headlessly (MCP, CLI, tests).

`pixors-desktop` is the **view+controller**. It renders `EditorState` using Iced and uploads tiles to the GPU atlas.

### Naming caveat

Types in `pixors-state` have "viewport" in their names (`TileCache`, `TileCacheSource/Sink`, `ViewportState`). This is legacy naming — they are actually general tile-cache and tile-range types. They do NOT depend on any display library.

### Decision test

> "Does this code need to know about Iced widgets, wgpu textures, GPU atlases, or file dialogs?"

- Yes → `pixors-desktop`
- No → `pixors-state` (if it's app/action logic) or a lower crate (if it's pure pipeline logic)

---

## Pipeline rules (non-negotiable)

1. **Processors never move data between CPU↔GPU.** The runtime injects `Upload`/`Download` automatically. Trust `context.device`.
2. **Processors never call wgpu directly.** All GPU work goes through `Scheduler`. No `wgpu::Device`, `wgpu::Queue`, or `wgpu::CommandEncoder` in a `Processor`.
3. **`context.device` is set by the compiler (`assign_devices`), not by the processor.**
4. **`Scheduler::download_buffer` does not exist.** Batch GPU→CPU is done by `DownloadProcessor`. Single reads use `Scheduler::read_from_buffer`.

---

## Action pattern

Every state mutation is an `Action`:

```rust
trait Action {
    fn prepare(&mut self, state: &mut EditorState) -> Result<PreparedAction, String>;
    fn apply(&mut self, state: &mut EditorState, status: ActionStatus);
    fn undo(&mut self, state: &mut EditorState);
    fn record_in_history(&self) -> bool;
}
```

`PreparedAction::StateOnly` — immediate, no pipeline.  
`PreparedAction::Pipeline { mode, graph, … }` — spawns pipeline thread. `mode` is `Background` (cancellable) or `Apply` (modal, locks tab).

---

## Key files

| File | Purpose |
|---|---|---|
| `pixors-engine/src/stage/node.rs` | `Stage` enum (`Producer`, `Processor`, `Consumer`), `StageHints` |
| `pixors-engine/src/stage/actors.rs` | `Producer`, `Processor`, `Consumer` traits |
| `pixors-engine/src/runtime/pipeline.rs` | `Pipeline::compile()`, device assignment, transfer insertion |
| `pixors-engine/src/gpu/scheduler.rs` | GPU API for processors |
| `pixors-engine/src/graph/graph.rs` | `ExecGraph` — owned graph with toposort |
| `pixors-state/src/editor.rs` | `EditorState` |
| `pixors-state/src/tab.rs` | `Tab` |
| `pixors-state/src/action/mod.rs` | `Action` trait, `PreparedAction` |
| `pixors-state/src/action/actions/` | Concrete actions: `OpenFile`, `Export`, … |
| `pixors-engine/src/graph/path_builder.rs` | `PathBuilder` — builds `ExecGraph` |
| `pixors-shader/src/kernel/` | `#[kernel]` annotated params structs (compose, blur, color, mip_downsample) |
| `pixors-shader-macro/src/lib.rs` | `#[kernel]` proc macro |
| `pixors-desktop/src/app.rs` | `App` struct (Iced) |
| `pixors-desktop/src/controller.rs` | `App::update()` — message routing |
| `pixors-desktop/src/viewport/tiled_texture.rs` | GPU texture atlas |
| `pixors-desktop/src/viewport/sink.rs` | `ViewportSink` — GPU→screen |

---

## Code style

- `cargo fmt --all` before commit.
- `cargo clippy --workspace` before push.
- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`.
- No comments unless the WHY is non-obvious.
- No extra abstractions beyond what the task requires.
