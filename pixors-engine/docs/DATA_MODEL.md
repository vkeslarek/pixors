# Data Model

Pixel representation, color space, bit depth, and alpha conventions for the working pipeline.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Channel layout | RGBA, always 4 channels |
| Memory layout (CPU/GPU) | Interleaved (`RGBA RGBA RGBA …`) |
| Memory layout (DISK) | Planar (matches TIFF convention) |
| Storage bit depth | 16-bit float (`f16` / half) |
| Compute bit depth | 32-bit float (`f32`), inside kernels |
| Alpha convention | Premultiplied (associated) |
| Working color space | ACEScg (AP1 primaries, D60, linear) |
| Input/output color spaces | Any — converted at load/save boundary |
| HDR tone mapping | Deferred (not in initial scope) |

## Channel layout

Always 4 channels: **R, G, B, A**. No grayscale-only or 3-channel optimizations. A grayscale image is RGBA with R=G=B and A=1.

Rationale: simplifies every operation (single code path), matches common GPU formats, matches the most common image ops (all-channel).

## Memory layout

**Interleaved in RAM and VRAM**: a tile buffer is a contiguous array of pixel structs, each 4 components side-by-side.

- Matches Vulkan storage buffer layout for `R16G16B16A16_SFLOAT`-equivalent packing
- Single allocation per tile
- Single upload/download per tile
- Most operations read/write all channels anyway

**Planar on DISK**: when writing TIFF/EXR and similar, store as planar if format supports it. On load, re-pack to interleaved. The planar↔interleaved conversion happens only at the DISK boundary.

## Bit depth

- **Storage**: `f16` (IEEE 754 half, ~10-bit mantissa, range ±65504)
- **Compute**: `f32` (IEEE 754 single, 23-bit mantissa)

**Rule**: kernels read `f16` → promote to `f32` for all math → write back `f16`. Every operation, no exceptions in the working pipeline.

Rationale:
- `f16` halves memory and bandwidth vs `f32` with acceptable precision for color
- `f32` compute avoids precision erosion in accumulative ops (repeated blur, multi-layer blend)
- Keeping the promote/demote inside the kernel means the I/O cost of `f32` never materializes in memory

### Load/save

Supported on-disk bit depths for import/export:

- 8-bit unsigned (`u8`)
- 16-bit unsigned (`u16`)
- 16-bit float (`f16`)
- 32-bit float (`f32`)

All convert to/from the internal `f16` interleaved representation.

## Alpha

**Premultiplied alpha (associated) internally, always.**

- RGB already multiplied by A in storage
- Blending (Porter–Duff) is correct by construction
- Filters (blur, downscale) do not leak color from transparent regions
- Industry-standard for compositing (Nuke, After Effects, Flame, Filament)

Formats that store straight (unassociated) alpha — notably PNG and most TIFF — are converted on load and save:

- **Load**: `RGB_straight * A → RGB_premul`
- **Save**: `RGB_premul / A → RGB_straight` (guard `A > ε` to avoid div by zero; clamp result)

The divide introduces quantization error only in the save path; internal round-trips stay in premultiplied.

## Color space

### Working space: ACEScg

- Primaries: **AP1** (Academy Color Encoding System, cinematic grade container)
- White point: **D60** (~6000 K)
- Transfer function: **linear** (no gamma)
- Gamut: wide, covers virtually all usable visual spectrum for pro workflows
- Encoding: scene-referred, allows values > 1.0 (HDR data)

Operations assume linear light. No hidden gamma.

### Input / output color spaces

Any image can be imported from any color space. The load pipeline:

1. Decode container (PNG, JPEG, TIFF, …) → raw pixels in source encoding
2. Apply inverse transfer function (gamma → linear)
3. Apply gamut conversion matrix (source primaries → AP1) with chromatic adaptation (Bradford CAT) if white points differ
4. Apply alpha premultiplication if needed
5. Store as `f16` interleaved in ACEScg linear premultiplied

Save pipeline is the inverse, in reverse order.

### Color space support strategy

Two-tier approach:

- **Hardcoded fast path** for the common list:
  - sRGB (IEC 61966-2-1)
  - Linear sRGB
  - Rec.709
  - Rec.2020
  - Adobe RGB (1998)
  - Display-P3
  - ProPhoto RGB
  - ACES2065-1 (AP0)
  - ACEScg (AP1) — working space, passthrough
- **Generic ICC profile path** for anything else, via a library (e.g. `lcms2` or `qcms`). Triggered when an embedded profile does not match any hardcoded entry.

On load: detect embedded ICC → if matches hardcoded ID, use fast path; else route through ICC engine.

### Quantization discipline

Conversion matrices introduce small errors. Rules:

- All conversion math in `f32`, regardless of storage format
- **Minimize round-trips**: exactly one conversion on load, one on save. No reconverting mid-pipeline
- **No-op when source CS == working CS**: skip conversion entirely (flag as passthrough)
- Gamma decode **before** primaries conversion (never convert primaries on non-linear data)

## Intermediate value caching

Two distinct cases produce "intermediate" tile storage:

### Case A — Working set exceeds GPU VRAM

An operation runs partly on GPU, but the full image does not fit. Tiles not currently on GPU live in CPU RAM. This is not really a cache problem — it is the normal three-tier model doing its job. See [STORAGE_ENGINES](STORAGE_ENGINES.md).

### Case B — Committed state retained for further editing

After a user action finishes, the resulting image state is kept because the user may edit more (undo, branching history, re-run with different parameters). Multiple committed states accumulate over a session.

**Storage strategy for committed states**:
- Primary representation remains `f16` interleaved tiles
- Cold states compressed with **LZ4** (patent-free, very fast decompress, decent ratio on image data)
- Hot state (currently editable) stays uncompressed
- Eviction / compression policy driven by editor history layer — see [EDITOR_SEMANTICS](EDITOR_SEMANTICS.md) once written

### Within a graph run

During graph execution, most transient `f32` values stay in registers through fusion. When an intermediate must materialize (fork in graph, non-fusible op, explicit preview), it goes to the normal storage engine in `f16`. No separate "graph cache" abstraction.

## HDR display

Values above 1.0 are valid in ACEScg (scene-referred). For SDR monitor display, a tone mapping operator will eventually be applied at the viewport stage only — never destructive. Not in the initial implementation.

## Related

- [OVERVIEW](OVERVIEW.md) — where data model fits in the bigger picture
- [STORAGE_ENGINES](STORAGE_ENGINES.md) — how tile buffers are allocated and moved
- [OPERATIONS](OPERATIONS.md) — how operations consume/produce this format
