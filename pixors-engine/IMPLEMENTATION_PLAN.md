# Pixors — Implementation Plan (Pragmatic)

> Fazer um editor de imagem maduro do zero é loucura. Este plano aceita essa realidade e quebra o trabalho em fases onde **cada uma entrega uma feature concreta**. A próxima só começa depois da anterior funcionar.

The design work lives in [`docs/`](docs/) and describes the ambitious end state. This document is the **execution path** — what is built first, in what order, and with what simplifications.

---

## Guiding Principles

1. **Faster feature wins over perfect feature**. A working half-slice ships; a half-built perfect slice does not.
2. **No early optimization**. Tiling, async, fused kernels, graph lowering — all valuable when they solve a real problem, not before. Keep each phase's implementation as simple as possible.
3. **Each phase has a runnable demo**. If it does not demo, it does not ship.
4. **Design docs are the reference**, not the plan. They describe where we want to go. This document describes how we walk there.
5. **Revise the plan as reality lands**. If a phase shows a later phase was wrong, rewrite the later phase. Don't build on a broken foundation.

---

## Phase Overview

| Phase | Goal | Runnable outcome |
|---|---|---|
| 1 | Image I/O abstraction | `pixors-cli` loads and saves PNG with correct color |
| 2 | GPU display | `pixors-view` opens a window and shows an image |
| 3 | Operations basics | `pixors-cli apply --brightness 1.2 …` works |
| 4 | Async engine | Per-tile parallel execution with cancellation |
| 5 | UI | Interactive editor with small op set |
| 6 | Editor semantics | Layers, masks, history |

Phases 1–3 are detailed below. Phases 4–6 are sketches, revisited after earlier phases land.

---

## Phase 1 — Image I/O Abstraction

### Goal

Build a clean abstraction for loading and saving images with correct color management. No GPU, no tiling, no async. Just in-memory `Image` structures and a module that reads/writes them.

### Why this first

- Everything else consumes / produces images. Getting the data model right early prevents costly rewrites
- Color management is the part most people get wrong — pin it down before it leaks into every other module
- Can be validated by round-trip PNG tests, no GPU or UI required

### Deliverables

- `Image` type — `f16` RGBA interleaved, premultiplied alpha, internally ACEScg linear
- `ColorSpace` enum — sRGB, Rec.709, Rec.2020, Adobe RGB, Display-P3, ProPhoto, ACES2065-1, ACEScg, Linear sRGB
- `TransferFunction` enum — sRGB gamma, Rec.709 gamma, gamma 2.2, linear
- Conversion pipeline — primaries matrix 3×3, Bradford CAT for white-point adaptation, gamma decode/encode
- PNG load — via `png` crate: decode u8/u16 → `f16` ACEScg premul
- PNG save — ACEScg premul → u8/u16 target space, unpremul for container
- Round-trip test — load → save → diff within quantization bounds
- CLI binary — `pixors-cli` with `--load` / `--save` plumbing for smoke tests

### Simplifications from full `docs/` design

- **Not tiled**. `Image` is one flat `Vec<f16>` of RGBA pixels. Tiling lands in Phase 4 when async/parallel demands it
- **Not MIP-aware**. No pyramid yet
- **Synchronous**. Load/save block the caller
- **No ICC profile fallback**. Hardcoded color spaces only. Generic ICC via `lcms2` deferred
- **No gamut mapping**. Save hard-clamps out-of-gamut values. Document this prominently
- **Single format — PNG**. JPEG, TIFF, EXR, WebP come later

### Task breakdown

1. **Cargo setup**
   - Dependencies: `png`, `half` (f16), `bytemuck`, `anyhow` or `thiserror`
   - Workspace with `pixors-core` lib crate and `pixors-cli` bin crate
2. **Core data types**
   - `struct Image { width: u32, height: u32, pixels: Vec<f16>, color_space: ColorSpace }`
   - `enum ColorSpace` with the 9 listed entries
   - `enum TransferFunction`
3. **Color math module** (`pixors_core::color`)
   - Const 3×3 primaries matrices per color space (canonical sources)
   - Bradford CAT for D65 ↔ D60 chromatic adaptation
   - Transfer-function decode/encode (sRGB piecewise, Rec.709, linear, gamma 2.2)
   - End-to-end pipeline: `convert(pixels, from: ColorSpace, to: ColorSpace)`
4. **Premul conversion**
   - `premultiply(&mut Image)` — multiply RGB by A
   - `unpremultiply(&Image) -> Image` — divide RGB by A with `α > ε` guard
5. **PNG load**
   - Decode PNG via `png` crate (u8, u16)
   - Detect embedded sRGB chunk / cICP / iCCP; default to sRGB if absent
   - Apply transfer-function decode
   - Apply primaries + CAT to ACEScg
   - Premultiply
   - Pack as `f16`
6. **PNG save**
   - Accept target `ColorSpace` param (default: sRGB)
   - Unpremultiply from ACEScg
   - Apply inverse primaries + CAT
   - Apply target transfer function
   - Hard-clamp to `[0, 1]`
   - Pack to target bit depth (u8 or u16), encode PNG
7. **Testing**
   - Synthesized test image with known colors
   - Round-trip: `load(save(load(fixture))) == load(fixture)` within tolerance
   - Cross-color-space round-trip: sRGB ↔ Rec.2020 ↔ back, verify no drift beyond quantization
   - Edge cases: fully transparent, fully opaque black, 1×1 image, very wide image
8. **CLI smoke test**
   - `pixors-cli convert --in photo.png --out out.png --target-space rec2020`

### Open questions to resolve during Phase 1

- Alpha-mode storage on `Image`: metadata field, or invariant "always premul internally" with conversion at boundaries? Leaning invariant-premul
- "Unknown color space" on load: assume sRGB by default (with a warning log) or require an explicit assertion? Leaning sRGB-with-warning
- Error type: `anyhow` for speed-of-iteration vs `thiserror` for typed handling. Decide upfront and stick

### Exit criteria

- `cargo test` passes all Phase 1 tests
- `pixors-cli convert` round-trips a real photograph without visible degradation
- Source reviewed for color-math correctness against a reference tool (e.g. ImageMagick `-colorspace` pipeline)

---

## Phase 2 — GPU Display

### Goal

Open a window and display a loaded `Image` on the GPU via Vulkan. Viewport supports pan and zoom. Used to eyeball-validate Phase 1 end-to-end.

### Why this second

- First proof the color pipeline is visually correct (if it displays wrong, the math is wrong)
- Forces us through the Vulkan setup boilerplate once, early, in a simple context
- Gives Phase 3+ a canvas for visualizing operation results

### Deliverables

- Vulkan instance + device via `ash`
- Swapchain + render pass
- Window via `winit`
- Image → `VkImage` upload, format `R16G16B16A16_SFLOAT`
- Shader pair (GLSL → SPIR-V at build time via `shaderc`) — vertex draws a quad; fragment samples the texture, converts ACEScg → sRGB, hard-clamps for display
- Mouse pan (drag) and zoom (wheel)
- CLI binary — `pixors-view <image-path>`

### Simplifications from full `docs/` design

- **No tiled upload**. Whole image → one `VkImage`
- **No MIP pyramid**. Rely on sampler filtering, or don't bother if image fits
- **No async upload**. Synchronous, block until uploaded
- **No HDR display**. Hard clamp stays ([D11](docs/DECISIONS.md#d11--hdr-tone-mapping-deferred))
- **One image at a time**

### Task breakdown

1. **Vulkan boilerplate**
   - Instance with validation layers in debug builds
   - Physical device enumeration; pick discrete GPU if present
   - Logical device with graphics + present queue (separate transfer queue deferred)
2. **Swapchain + render pass**
   - Surface via `winit` raw window handle
   - Swapchain format: sRGB on swapchain side, shader writes sRGB-encoded values
   - Minimal render pass: one color attachment, clear + store
3. **Image upload path**
   - Staging buffer (host-visible) → `VkImage` (device-local, `R16G16B16A16_SFLOAT`)
   - Transfer via graphics queue (combined queue fine Phase 2)
   - Layout transitions: `UNDEFINED → TRANSFER_DST → SHADER_READ_ONLY`
4. **Shader pipeline**
   - Vertex: full-screen quad with UV output
   - Fragment: sample texture (interpreted as ACEScg f16), convert to sRGB (matrix + gamma encode), clamp, output
   - Compile GLSL → SPIR-V at build time via `build.rs` + `shaderc`
5. **Viewport state**
   - Struct holding `pan: (f32, f32)`, `zoom: f32`
   - Winit mouse event handlers: left-drag updates `pan`, wheel updates `zoom`
   - Vertex shader consumes pan/zoom via push constant
6. **Event loop**
   - Winit event loop; redraw on window damage or on pan/zoom change
   - Resize handling (recreate swapchain)
7. **CLI smoke test**
   - `pixors-view photo.png` opens a window, image visible, pan + zoom work

### Open questions to resolve during Phase 2

- `ash` confirmed as Vulkan binding (vs `vulkano`, `wgpu`). Design doc baseline — stay unless Phase 2 exposes real pain
- Shader language: GLSL via `shaderc` (closes [REVIEW](docs/REVIEW.md) shader-language open question for Phase 2+). Revisit WGSL/naga only if `shaderc` becomes awkward
- Wayland vs X11: `winit` handles both, verify on Linux target

### Exit criteria

- `pixors-view` displays a loaded image correctly compared to a browser or reference viewer
- Panning and zooming smooth (no visible stutter at reasonable image sizes)
- No Vulkan validation-layer errors in debug builds

---

## Phase 3 — Operations Basics

### Goal

Define the operation abstraction and ship a small set of CPU-only ops that apply to an `Image`. Execution is **synchronous and immediate** — no graph, no lazy evaluation, no `ctx.run()`. User writes `image = brightness(image, 1.2);` and gets a new image back.

### Why this third

- Needs Phase 1 done (Image I/O)
- Benefits from Phase 2 to visually inspect op results
- Keeps scope tiny on purpose — validates the op-trait design without async/tiling complexity

### Deliverables

- `Operation` trait (Phase 3 shape — simpler than full [OPERATIONS.md](docs/OPERATIONS.md))
- Op implementations (CPU, whole-image): `brightness`, `contrast`, `gamma`, `invert`, `gain`, `color_matrix`, `premul`, `unpremul`
- Tests per op — snapshot against known-fixture inputs
- CLI binary — `pixors-cli apply --brightness 1.2 --contrast 1.1 input.png output.png`
- Optional: `pixors-view` gains runtime op controls to eyeball results (nice-to-have)

### Simplifications from full `docs/` design

- **No lazy graph**. Ops execute immediately. No `ValueId`, no `Context`, no `ctx.run()`
- **CPU only**. GPU kernel trait arrives in Phase 4 with tiling
- **No neighborhood ops**. Gaussian blur, box blur, sharpen come in Phase 4 when tiling supports proper work-unit dispatch
- **No meta-producing ops**. Histogram, statistics, interest points in Phase 4+
- **Scalar params, no Meta nodes**. `brightness(image, 1.2_f32)` takes a plain Rust value, not a `ValueId<Meta<f32>>`
- **No fusion**. Each op call allocates a new `Image`; repeated calls chain trivially. Cost is noise on small images; revisit in Phase 4 if needed
- **No MIP awareness**. Ops run on the whole image at native size

### Phase 3 Operation trait (draft)

Simplest useful contract:

```text
trait Operation {
    type Params;
    fn apply(&self, input: &Image, params: &Self::Params) -> Result<Image>;
}
```

Each op is a zero-sized type with its own params struct. No trait-object magic, no dispatch tables. A helper module (`pixors_core::ops`) exposes ergonomic free functions:

```text
fn brightness(image: &Image, factor: f32) -> Result<Image>;
fn contrast(image: &Image, factor: f32) -> Result<Image>;
// ...
```

The trait is present so Phase 4 migration is smooth (same shape, plus tiling / engine selection / async). Users mostly call the free functions in Phase 3.

### Task breakdown

1. **Op trait + error type**
   - Define `Operation` trait as above
   - Extend Phase 1 error type with op-level errors (`InvalidParam`, etc.)
2. **Per-op implementations**
   - Iterate over `Vec<f16>` as `[f16; 4]` chunks
   - Promote to `f32` for compute ([D8](docs/DECISIONS.md#d8--storage-f16-compute-f32)), write back `f16`
   - Preserve premultiplied invariant
   - Ops:
     - `brightness(α)` — `rgb *= α`
     - `contrast(β)` — `rgb = (rgb - 0.5) * β + 0.5` (pre/post-demul as needed)
     - `gamma(γ)` — `rgb = rgb.powf(γ)` (requires temporary unpremul)
     - `invert` — `rgb = 1.0 - rgb` (requires unpremul/repremul)
     - `gain([r, g, b, a])` — per-channel multiply
     - `color_matrix([[m..]; 4])` — 4×4 matrix apply
     - `premul` / `unpremul` — straight conversion
3. **Testing**
   - Synthesized fixtures: solid colors, gradients, alpha-varying patches
   - Snapshot tests: known input → known output
   - Property tests: `invert(invert(x)) ≈ x`, `premul ∘ unpremul ≈ identity` (within quantization)
4. **CLI pipeline**
   - `pixors-cli apply` accepts multiple `--op` flags, applies in order
   - Logs the pipeline composed
5. **Optional visual loop** (nice-to-have)
   - `pixors-view` side panel with sliders, re-apply op + re-upload image — immediate feedback, synchronous, low-tech

### Open questions to resolve during Phase 3

- Does `contrast` operate premul-aware or require unpremul/repremul? Unpremul wrap safer default for ops breaking the premul invariant; document per op
- CLI argument parsing: `clap` (standard) vs hand-rolled. `clap` probably
- Allocate fresh `Image` per step or in-place? Fresh per step in Phase 3 — optimization is Phase 4

### Exit criteria

- All ops pass snapshot tests
- `pixors-cli apply` runs a multi-op chain on a real photograph, produces a visually correct result
- Phase 3 codebase is the skeleton Phase 4 extends — trait signature stays, only widens

---

## Phase 4 — Async Engine per Tile (sketch)

_Revisited in detail after Phase 3 ships._

This is where most of the `docs/` design becomes real. Candidate scope:

- Migrate `Image` to tiled representation (256×256, per [D13](docs/DECISIONS.md#d13--tile-size-256256))
- Extend `Operation`: tile + neighborhood declaration
- Two scheduling strategies to benchmark:
  - **Per-tile pipeline**: for each tile, run the whole pipeline end-to-end
  - **Per-op broadcast**: run op 1 on all tiles, then op 2, etc.
  - Measure real workloads, pick the one that actually performs better
- Async execution on a thread pool (`smol` / `tokio` — decide at implementation time)
- Cancellation token per job, checked at work-unit boundary
- Begin MIP pyramid generation (lazy, only when something needs a lower-res version)
- Begin viewport-driven priority (tiles on screen first)
- Introduce `Context` / `ValueId` lazy graph — keep simple; fusion defers
- Add neighborhood ops (Gaussian blur, box blur) to exercise work-unit formation

### Risks to watch

- Tiling pervasively risks rewriting most of Phase 1–3. Migration plan matters — consider an adapter keeping flat `Image` as a façade
- Correctness: tiled blur with neighborhood must produce bit-identical output to un-tiled blur. Headline test
- Overhead: tile overhead should not slow down small-image workloads vs Phase 3's flat pipeline. Benchmark both

---

## Phase 5 — UI (sketch)

_Revisited after Phase 4._

- UI framework: `egui` leading candidate (good Vulkan integration, immediate-mode fits interactive editing)
- Viewport widget with proper structure (not just drag/zoom)
- Side panels: op controls, histogram preview, layer / session info
- File menu: open, save, save-as
- Keyboard shortcuts
- Real-time op application — slider moves → re-apply — tests Phase 4's cancellation path

---

## Phase 6 — Editor Semantics (sketch)

_Revisited after Phase 5._

- Layer stack + blend modes (Porter–Duff)
- Layer masks (pixel + vector)
- Selections (rect, lasso, color-range)
- Non-destructive adjustment layers
- Undo/redo history
- Session persistence (`.pixors` file)

---

## Design Doc References

The detailed architecture is in [`docs/`](docs/). This plan refers to it rather than duplicating it.

- [Overview](docs/OVERVIEW.md) — principles and scope
- [Data Model](docs/DATA_MODEL.md) — pixel format, color, alpha
- [Storage Engines](docs/STORAGE_ENGINES.md) — three-tier storage model
- [Tile System](docs/TILE_SYSTEM.md) — tiles, work units
- [MIP Pyramid](docs/MIP_PYRAMID.md) — multi-resolution pyramid
- [Operation Graph](docs/OPERATION_GRAPH.md) — lazy graph
- [Operations](docs/OPERATIONS.md) — op trait, catalog, MIP scaling
- [Execution Model](docs/EXECUTION_MODEL.md) — two-level IR
- [Scheduler](docs/SCHEDULER.md) — priority, queues
- [Decisions](docs/DECISIONS.md) — locked decisions log (D1–D55+)
- [Roadmap](docs/ROADMAP.md) — earlier phased roadmap (superseded by this plan for ordering)
- [Review](docs/REVIEW.md) — design review: gaps, simplifications, risks

The docs describe the end state. This plan describes the incremental path.

---

## Iteration Policy

After each phase lands:

1. Run the phase's exit criteria
2. Revisit the next phase — scope still right given what we learned?
3. Look at [REVIEW.md](docs/REVIEW.md) Tier 1 items: any applying to the next phase must be resolved before starting it; others defer
4. Update this document and [ROADMAP](docs/ROADMAP.md) to reflect reality
5. If a design doc is wrong, fix it — don't perpetuate the error

Features that "sound good" but don't contribute to the current phase deliverable get deferred, no exceptions.

---

## Current Status

- Phase 0 (design docs): **done** — files under `docs/` populated, 55+ decisions locked, Review round 1 complete
- Phase 1: **not started** — next up
- Source in `src/` is a scaffold only; will be restructured as Phase 1 begins
- `Cargo.toml` currently pulls Vulkan/winit for early prototyping; will be re-scoped per phase
