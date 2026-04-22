# Operations

Per-op contracts: signature, neighborhood, engine kernels, MIP scaling, boundary policy, cancellation. Catalog of Phase 1 MVP operations.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Kernel source | Pair of kernels per op — CPU function + GPU shader, each via its own trait |
| Trait structure | `CpuKernel` and `GpuKernel` are separate traits; an op implements either, both, or fails compilation |
| Shader language | **Open question** (GLSL leading, WGSL / HLSL on the table) |
| Neighborhood | Declared by the op as a function of its Meta inputs (can be dynamic) |
| Boundary policy | Declared per op; no global default |
| MIP scaling | Op-owned — operation computes scaled parameters itself; no automatic engine scaling |
| Cancellation | Cooperative per work unit — work units check a token before dispatch |
| Image file I/O | Use established crates (do not reimplement format parsing) |
| Interest-point algorithms | Implemented in-tree, no OpenCV dependency |

## Op anatomy

An operation is a named thing that declares:

- **Input signature** — ordered list of `(name, type)` where type is `Image`, `Meta<T>`, or `MetaBuffer<T>`
- **Output signature** — same shape
- **Neighborhood function** — given the Meta inputs, returns a rectangular `Neighborhood { left, right, top, bottom }`. For ops whose neighborhood does not vary, the function is a constant
- **Boundary policy** — how the kernel treats out-of-image reads (clamp / mirror / wrap / zero / transparent)
- **MIP scaling rule** — for every parameter the op uses, how the parameter transforms when running at MIP level `n`. The op exposes a `scale_for_mip(n, inputs) -> scaled_inputs` step that runs before the kernel
- **CPU kernel** (optional) — pure Rust implementation
- **GPU kernel** (optional) — shader source compiled to SPIR-V at build time

At least one of CPU/GPU kernel must be present. If an op only declares the CPU kernel, it is CPU-only (see [D4](DECISIONS.md#d4--operation-capabilities-are-opt-in-per-engine)) and the scheduler will not attempt GPU dispatch.

## Kernel traits

Two separate traits, not one with optional methods:

### `CpuKernel`

A CPU kernel:
- Takes the op's inputs (tile buffers as `&[F16Pixel]`, neighbor tiles, meta values by reference)
- Writes into an output tile buffer
- Is a plain Rust function — free to use SIMD, rayon, tight loops, etc
- Runs on the engine's CPU thread pool

### `GpuKernel`

A GPU kernel:
- Provides SPIR-V bytecode (precompiled at build time from the chosen shader source)
- Declares its binding layout (storage buffer for tile, storage buffer for neighbors, uniform buffer for `Meta`, storage buffer for `MetaBuffer`)
- Runs as a Vulkan compute dispatch on the engine's GPU queue

### Separation

An op implementing only `CpuKernel` is CPU-only. An op implementing only `GpuKernel` is GPU-only. An op implementing both is adaptively dispatched by the scheduler (see [SCHEDULER](SCHEDULER.md) _(TBD)_).

This matches [D4](DECISIONS.md#d4--operation-capabilities-are-opt-in-per-engine) exactly: no empty-stub implementations, no runtime "not implemented" errors.

## Neighborhood — dynamic per Meta

Operations whose reach depends on a parameter (any blur, sharpen, convolution with variable kernel) compute their neighborhood from the current Meta inputs:

- `gaussian_blur(image, radius: Meta<f32>)` → neighborhood = `{ left: radius, right: radius, top: radius, bottom: radius }`
- `box_blur(image, size: Meta<u32>)` → same pattern

Evaluation order in the scheduler:
1. Meta input values are materialized (already-computed upstream or folded constants)
2. Op evaluates its neighborhood function with those values
3. Work units are formed with that neighborhood
4. Kernels dispatch

The neighborhood is **recomputed** if a fresh Meta flows in (e.g. the user moves a slider). This interacts with cancellation (below): a new job replaces the old with a new neighborhood and a new set of work units.

## Boundary policy

Each op declares its policy for reads outside the image boundary. No global default — the op is responsible because the correct choice depends on the op's meaning.

Available policies:

- **Clamp** — read the nearest valid edge pixel
- **Mirror** — reflect across the boundary
- **Wrap** — toroidal, wrap to the other side
- **Zero** — read `(0, 0, 0, 0)` (with premultiplied alpha, same as "fully transparent")
- **Transparent** — explicit `(0, 0, 0, 0)`; identical to zero under premultiplied convention but labelled for intent

Typical choices:
- Gaussian blur, box blur: **mirror** (avoids dark halos at edges)
- Edge detect (Sobel / DoG): **clamp** (avoids spurious edges at image borders)
- Composition / fill: **zero**
- Tiling patterns: **wrap**

## MIP scaling — op-owned

Every op must produce a consistent result at any MIP level ([D29](DECISIONS.md#d29--every-operation-must-be-mip-aware)). How a param transforms across MIP levels is **op-specific**, not automatic. Examples:

- **Gaussian blur** `sigma`: divide by `2^n` at MIP `n` — naively, but sigma interacts with the Gaussian kernel non-linearly. The op may choose `sigma_at_n = sqrt(sigma_0² / 2^(2n))` or a clamped version, or fall back to "run at MIP 0 only" when sigma drops below a useful threshold
- **Box blur radius**: divide by `2^n`, rounded; clamp to at least 1 if scaling would collapse to 0
- **Color matrix / gain / gamma**: invariant — same math at every level
- **Geometric translate** (pixel offset): divide by `2^n`
- **Warp** homography: scale the translation components; rotation/perspective unchanged

The op's `scale_for_mip(n, inputs)` runs **before** any work-unit formation. If the op decides a MIP level is not usefully computable (e.g. blur sigma below pixel precision), it can **declare** so, and the scheduler falls back to running at MIP 0 and composing up ([D29](DECISIONS.md#d29--every-operation-must-be-mip-aware) worst-case path).

## Cancellation

### Rationale

Interactive editing floods the engine with jobs: every slider tick is a new graph run on the same source. Stale jobs must drop fast so the latest input wins.

Canonical UX flow:
1. User drags a slider
2. Each tick creates a new job with updated Meta
3. The new job **cancels** the previous job with the same output target
4. Work units already dispatched finish (safely, no half-written tiles); work units not yet dispatched are skipped

### Contract — cooperative per work unit

- Every job carries a cancellation token
- Before dispatching a work unit, the scheduler checks the token
- Work units mid-flight are not killed — Vulkan dispatches run to completion; CPU kernels run their loop to completion
- Granularity = one work unit. This gives bounded tail latency (one work unit's runtime)

### What cancel means for tile state

- A cancelled work unit does not write to the output tile
- Partially-completed cancelled jobs leave the image in the **previous committed state**; no rollback needed because the new job writes fresh outputs
- If a cancelled job already wrote some viewport tiles before cancellation, those tiles are overwritten by the new job as it runs

### What cannot be cancelled

- The currently-executing work unit (would require per-pixel abort, not worth the cost)
- In-flight transfers (Vulkan doesn't expose cheap mid-transfer abort)
- I/O reads (let them finish; result is just discarded)

## Op catalog — Phase 1 MVP

Phase 1 targets a complete smoke-test pipeline end to end: load, a few edits, save; plus a couple of metadata pipelines used by the image-alignment example.

### File I/O

| Op | In | Out | Notes |
|---|---|---|---|
| `load_png` | path (host) | `Image` | uses the `png` crate; decode → premul → ACEScg |
| `save_png` | `Image`, path | — | unpremul → encode via `png` |

Later phases add JPEG, TIFF, EXR. Load/save obeys the color-space conversion rules from [DATA_MODEL](DATA_MODEL.md).

### Pixel (no neighborhood)

| Op | In | Out | MIP scaling |
|---|---|---|---|
| `brightness` | `Image`, `Meta<f32>` factor | `Image` | invariant |
| `contrast` | `Image`, `Meta<f32>` factor | `Image` | invariant |
| `gamma` | `Image`, `Meta<f32>` | `Image` | invariant |
| `gain` | `Image`, `Meta<[f32; 4]>` per-channel | `Image` | invariant |
| `invert` | `Image` | `Image` | invariant |
| `color_matrix` | `Image`, `Meta<[f32; 16]>` (4×4) | `Image` | invariant |
| `premul` | `Image` | `Image` | invariant |
| `unpremul` | `Image` | `Image` | invariant |

Per-pixel ops are the ideal fusion candidates ([D34](DECISIONS.md#d34--fusion-baseline-same-engine--same-tile-topology--single-kernel)). A chain of these compiles to one kernel.

### Local (rectangular neighborhood)

| Op | In | Out | Neighborhood | Boundary |
|---|---|---|---|---|
| `gaussian_blur` | `Image`, `Meta<f32>` sigma | `Image` | `ceil(3σ)` each side | mirror |
| `box_blur` | `Image`, `Meta<u32>` radius | `Image` | `radius` each side | mirror |

### Geometric

| Op | In | Out | Notes |
|---|---|---|---|
| `crop` | `Image`, `Meta<Rect>` | `Image` | re-tiles; changes image dimensions |

### Metadata producers

Reductions from images into small results. Needed for the alignment example and many adaptive pipelines.

| Op | In | Out | Notes |
|---|---|---|---|
| `histogram` | `Image` | `Meta<Histogram>` (256-bin per channel) | GPU with atomics or CPU reduce |
| `image_statistics` | `Image` | `Meta<Stats>` (min/max/mean/stddev per channel) | GPU parallel reduction or CPU |
| `find_interest_points` | `Image`, `Meta<DetectorParams>` | `MetaBuffer<KeyPoint>` | implemented in-tree (ORB, no OpenCV dep) |
| `match_points` | `MetaBuffer<KeyPoint>`, `MetaBuffer<KeyPoint>` | `MetaBuffer<Match>` | brute-force or FLANN-style, in-tree |
| `compute_transform` | `MetaBuffer<Match>`, `Meta<TransformKind>` | `Meta<Transform>` | RANSAC + linear solver, in-tree |

### Not in Phase 1 MVP

Deferred to later phases (see [ROADMAP](ROADMAP.md)):

- **Compositing**: `blend` (Porter–Duff modes), `apply_mask`, `layer_merge`
- **Geometric (full)**: `resize`, `rotate`, `warp_perspective`, `warp_affine`
- **Masks / selections**: `threshold`, `feather`, `grow`, `shrink`
- **Non-local ops**: `curves`, `levels` (need lookup tables), `denoise`
- **Frequency domain**: FFT, convolution-via-FFT
- **Edge detection suite**: Sobel, Canny, DoG — arrive with the alignment pipeline expansion

## Library choices

### Image format parsing — use established crates

Do not reimplement format parsers. Choices:

- `png` — ubiquitous, well-maintained
- `jpeg-decoder` / `image` — JPEG read/write
- `tiff` — TIFF reader (write may need `tiff-encoder` or custom)
- `exr` — OpenEXR
- `webp` — optional later

`image` crate bundles many formats but adds weight; we can use specific crates and assemble the container-layer ourselves.

### Color management

- `lcms2` or `qcms` — generic ICC fallback path (see [D12](DECISIONS.md#d12--icc-profiles-hardcoded-fast-path--generic-fallback))
- Hardcoded conversions written in-tree

### Interest points and matching — implemented in-tree

- ORB (Oriented FAST + Rotated BRIEF) — patent-free (BRIEF is not patented; ORB itself is free)
- Brute-force Hamming distance matcher for binary descriptors
- RANSAC estimator for homography / affine

Rationale: OpenCV brings a large C++ dependency chain for what is a small set of algorithms. Rust implementations of these fit on a few hundred LoC each and avoid the FFI surface.

### GPU compute tooling

- `ash` — Vulkan bindings
- `shaderc` or `naga` — shader compilation (decision depends on chosen shader language)
- `gpu-allocator` (optional) — if suballocation becomes needed later

## Open questions

- **Shader language** — GLSL / WGSL / HLSL. Preference leans GLSL (Vulkan-native, `shaderc` mature). WGSL via `naga` is cleaner syntactically and cross-API. Pending decision
- **Fusion codegen** — how does a fused chain of per-pixel ops actually emit a combined shader or CPU loop? Proc macro? Manual template? String concat? To be discussed when implementing Phase 2

## Relations

- [OPERATION_GRAPH](OPERATION_GRAPH.md) — how ops combine into a graph
- [DATA_MODEL](DATA_MODEL.md) — pixel / channel layout every op obeys
- [MIP_PYRAMID](MIP_PYRAMID.md) — MIP-awareness contract
- [SCHEDULER](SCHEDULER.md) — cancellation delivery, engine routing _(TBD)_
- [EXECUTION_MODEL](EXECUTION_MODEL.md) — work-unit formation driven by these declarations _(TBD)_
