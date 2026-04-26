# Backend Refactor Plan — pixors-engine

Author target audience: an implementing model that has never seen this codebase.
Reading order: top to bottom. Each section is self-contained; do not skip the **Why** lines — they justify the design choice and let you handle edge cases.

---

## 0a. Revision history

- **v1** — initial plan.
- **v2** (this revision) — addresses critique: typed-row fast path narrowed to planar, u16 decode strategy made explicit (formula, no 256 KB LUT), `SrcReader` vs `SrcPixel` split, premultiplication moved to a per-call parameter, dispatch path from runtime layout to static generic spelled out, `convert_region` added, `Tile::to_srgb_u8` return type fixed, `BandBuffer` deletion made explicit, golden bit-exact regression test mandated, file split (conversion.rs stays focused; pipeline lives in `convert/pipeline.rs`, tile streaming in `convert/tile_stream.rs`).

## 0. Guiding principles

1. **The simplest solution is always the best.** Prefer one generic primitive over six specialized ones.
2. **One name, one responsibility.** If two functions do the same job for different concrete types, they should be one generic function.
3. **Push specialization to the leaves, not the trunk.** Pixel-/format-specific code lives in small `impl` blocks; the conversion pipeline is generic.
4. **Don't add abstraction unless it removes duplication.** Every new trait must replace ≥2 existing duplicates. If only one impl exists today, inline it.
5. **No backwards-compatibility shims.** Delete old method names; do not keep wrappers.
6. **Keep changes mechanical and isolated.** Each step compiles and passes tests on its own. Do not bundle unrelated cleanups.

---

## 1. Current pain points (read this first)

Re-read these files before starting. They are the root cause of the duplication:

- `src/convert/simd.rs` — two ad-hoc functions hardcoding `Rgba<f16>`/sRGB-u8 and `Buffer→Rgba<f16>`/ACEScg.
- `src/color/conversion.rs` — `ColorConversion` only knows about decoding **u8**. Encoding LUT is `f32→f32`, not `f32→u8` or `f32→u16`. No row/buffer entry points.
- `src/image/buffer.rs` — `ImageBuffer::read_sample` dispatches a `match` on `ComponentEncoding` per sample. f32 path needlessly clamps. u16 path forces big-endian.
- `src/io/mod.rs` — `stream_image_buffer_to_tiles` allocates a full-width band of `Rgba<f16>` then copies tile-strided slices out.
- `src/io/png.rs`, `src/io/tiff.rs` — both `stream_to_tiles_sync` impls call `load_png`/`load_tiff` (full decode) then `stream_image_buffer_to_tiles`. The "streaming" interface is a fiction.
- `src/image/typed.rs` — `read_region` is scalar per-pixel. `from_raw` always converts to ACEScg; the type is locked to `Rgba<f16>`. The lazy/typed split duplicates everything in `simd.rs`.
- `src/image/tile.rs` — `Tile<Rgba<f16>>::to_srgb_u8` wraps `acescg_f16_to_srgb_u8_simd`. Same ad-hoc binding.
- `src/storage/tile_store.rs` — `read_tile` clones the Arc'd Vec into a fresh `Tile`. Serialization is per-pixel; on LE hosts it could be a single memcpy/`bytemuck::cast_slice`.

---

## 2. Target architecture (what we are building toward)

```
                                          ┌──────────────────────────┐
                                          │ ColorConversion          │
                                          │   src ColorSpace         │
ImageBuffer (any layout, any sample type) │   dst ColorSpace         │  Vec<P> (any pixel type)
   ─────────────────────────────────────► │   matrix Matrix3x3       │ ─────────────────►
                                          │   decode LUT (per src)   │
                                          │   encode LUT (per dst)   │
                                          │                          │
                                          │   convert_row<DstP>(...) │
                                          │   convert_buffer<DstP>() │
                                          └──────────────────────────┘
```

Single converter type. Single SIMD pipeline. The pipeline is parameterized by **source layout** (read via `ImageBuffer`) and **destination pixel type** (a small `DstPixel` trait with `pack` + `unpack`). No `convert_X_to_Y` functions exist anywhere outside that module.

---

## 3. Refactor steps

Each step lists **What**, **Where**, **Why**, **How to apply**. Implement steps in order. Run `cargo test --workspace` after each.

### Step 1 — Make `ImageBuffer` sampling cheap and correct

**Where:** `src/image/buffer.rs`.

**What:**
1. Replace `ComponentEncoding` with:
   ```rust
   pub enum SampleFormat { U8, U16Le, U16Be, F16Le, F16Be, F32Le, F32Be }
   ```
   `UnsignedInt` vs `UnsignedNormalized` was never meaningful — the engine always wants normalized `[0,1]`. Drop the distinction; normalize at read.
2. Remove the `f32`-path `clamp(0.0, 1.0)` in `read_sample`. HDR float buffers must pass through unclamped.
3. Add **two** typed-row helpers on `PlaneDesc`:
   ```rust
   /// Planar layout: stride == size_of::<T>(). Returns whole row as &[T].
   pub fn planar_row<'a, T: Pod>(&self, data: &'a [u8], y: u32) -> Option<&'a [T]>;

   /// Interleaved layout: stride == N * size_of::<T>() (where N = channels in the pixel).
   /// Returns the whole row as &[T] and the caller picks via `chunks_exact(N)` + offset.
   pub fn interleaved_row<'a, T: Pod, const N: usize>(&self, data: &'a [u8], y: u32) -> Option<&'a [T]>;
   ```
   Both return `None` if alignment/stride doesn't match. The planar variant covers gray and EXR-style separated planes; the interleaved variant covers RGBA8/RGB8/RGBA16 (the common cases). Without the interleaved helper the fast path never fires for the most frequent layout.
4. Add `BufferDesc::is_planar(&self) -> bool` and `is_interleaved_packed(&self) -> bool` so the pipeline can pick a fast path.

**Why:** the per-sample `match` in `ComponentEncoding::read_sample` is the inner loop. SIMD must access a typed `&[T]` row when the layout permits. The original critique is correct: `stride == size_of::<T>()` alone never matches RGBA8 interleaved (stride=4, T=u8, size=1). The `N`-aware interleaved helper fixes that. Big-endian default on u16 forces TIFF to byteswap native u16 → BE → f32 — pure waste. Make endianness explicit; the host writes native.

**How to apply:** TIFF reader: replace `v.to_be_bytes()` with `v.to_ne_bytes()` and tag the plane `SampleFormat::U16` matching `cfg(target_endian)`. PNG already produces 8-bit native bytes; classify as `U8`. The scalar `read_sample` path stays as a fallback for non-aligned/exotic layouts; the typed-row helpers are the fast path the pipeline uses when available.

---

### Step 2 — Make `ColorConversion` the universal converter

**Where:** `src/color/conversion.rs`.

**What:** extend `ColorConversion` so it owns decode→matrix→encode for any source layout and any destination pixel type. **No `decode(x)` wrapper is added** — `self.src.transfer().decode(x)` already exists. Don't duplicate.

```rust
pub struct ColorConversion {
    src: ColorSpace,
    dst: ColorSpace,            // NEW: was missing; needed for diagnostics
    matrix: Matrix3x3,
    decode_u8: Box<[f32]>,      // 256 entries
    encode: Box<[f32]>,         // 4096 entries, dst-encoded f32
    // No decode_u16 LUT. See "u16 decode strategy" below.
}

impl ColorConversion {
    /// Convert one row of an ImageBuffer into a destination slice.
    /// `dst.len() == buf.desc.width as usize`. `mode` controls premultiplication.
    pub fn convert_row<D: DstPixel>(
        &self,
        buf: &ImageBuffer,
        y: u32,
        dst: &mut [D],
        mode: AlphaPolicy,
    );

    /// Convert a rectangular region into a fresh Vec.
    /// Used by viewport tile reads — the missing `convert_region` from v1.
    pub fn convert_region<D: DstPixel>(
        &self,
        buf: &ImageBuffer,
        x: u32, y: u32, w: u32, h: u32,
        mode: AlphaPolicy,
    ) -> Vec<D>;

    /// Convert the whole image. Rayon-parallel over rows.
    pub fn convert_buffer<D: DstPixel>(&self, buf: &ImageBuffer, mode: AlphaPolicy) -> Vec<D>;

    /// Convert a flat slice already in the src ColorSpace (typed source pixels).
    /// Used by Tile<Rgba<f16>>::to_srgb_u8.
    pub fn convert_pixels<S: SrcSlicePixel, D: DstPixel>(&self, src: &[S], mode: AlphaPolicy) -> Vec<D>;
}

pub enum AlphaPolicy {
    /// Output is premultiplied: pack writes `(r*a, g*a, b*a, a)`.
    PremultiplyOnPack,
    /// Output is straight: pack writes `(r, g, b, a)`. RGB already unpremultiplied if input was premul.
    Straight,
    /// No alpha in destination: pack writes `(r, g, b)`.
    OpaqueDrop,
}
```

**u16 decode strategy:** **do not build a 65 536-entry LUT** (256 KB allocation per converter is unjustified for the common case where the source u16 is linear or has a simple transfer). Instead, branch in the inner loop:

```rust
fn decode_sample(&self, raw: f32, fmt: SampleFormat) -> f32 {
    match fmt {
        SampleFormat::U8 => self.decode_u8[raw as u8 as usize], // raw is already 0..=255
        _ => self.src.transfer().decode(raw),                   // formula path
    }
}
```

For PNG (always u8 today) the LUT path runs. For TIFF u16/f16/f32 the formula path runs — measure before optimizing. If profiling shows a u16-heavy workload becoming the bottleneck, add the LUT later behind a feature flag. This sidesteps both the `&self` mutability problem and the wasted memory for one-shot conversions.

**Encode LUT:** keep one `encode: Box<[f32]>` (4096 entries, dst-encoded float). `DstPixel::pack` does the final scale (`* 255` or `* 65535`) plus rounding. Avoids per-dst-integer-width LUT duplication.

**Decoupling buffer-source vs typed-source:** the v1 plan conflated two things into one `SrcPixel` trait. Split:
- `SrcReader` (next step) — knows how to gather 4 lanes of (r,g,b,a) from an `ImageBuffer` row at column `x`.
- `SrcSlicePixel` (next step) — knows how to unpack one already-typed pixel (e.g. `Rgba<f16>` premultiplied) into `[f32; 4]` straight RGBA.

`convert_row`/`convert_region`/`convert_buffer` use `SrcReader`; `convert_pixels` uses `SrcSlicePixel`.

**Why:** the converter is the right home for the conversion API. The critique was correct that `decode(x)` is a redundant wrapper, that the u16 LUT decision was hand-wavy, that `&self` mutability would block `OnceLock`-based lazy build, and that `PREMULTIPLIED` belonged on the call, not the type. All addressed.

**How to apply:** add the `dst` field to `ColorConversion::new` (mechanical). Build encode LUT as today. Implement `convert_row` first (smallest unit). Build `convert_region` and `convert_buffer` on top. `convert_pixels` is structurally similar but reads from `&[S]` instead of `ImageBuffer`. Keep the existing `decode_to_linear`, `decode_u8_to_linear`, `encode_fast`, `matrix` methods until call sites migrate; delete in Step 9.

---

### Step 3 — Reader / Pixel traits (the SIMD glue)

**Where:** new file `src/convert/pipeline.rs` (replaces `src/convert/simd.rs`).

**Three traits, three jobs:**

```rust
/// Reads 4 lanes of (r,g,b,a) from an ImageBuffer row at columns x..x+4.
/// Returns alpha = 1.0 for layouts without alpha (gray, rgb).
pub trait SrcReader {
    fn read_x4(buf: &ImageBuffer, x: u32, y: u32) -> (f32x4, f32x4, f32x4, f32x4);
    fn read_one(buf: &ImageBuffer, x: u32, y: u32) -> [f32; 4];
}

/// Unpacks one already-typed pixel (e.g. premultiplied Rgba<f16>) into straight [f32; 4] RGBA.
pub trait SrcSlicePixel: Copy + bytemuck::Pod {
    /// Returns straight (unpremultiplied) RGBA in linear src space.
    fn unpack(self) -> [f32; 4];
    fn unpack_x4(s: &[Self]) -> (f32x4, f32x4, f32x4, f32x4); // s.len() >= 4
}

/// Packs 4 lanes (post-matrix, post-encode-LUT linear) into the destination pixel type.
pub trait DstPixel: Copy + bytemuck::Pod {
    fn pack_x4(rr: f32x4, gg: f32x4, bb: f32x4, aa: f32x4, mode: AlphaPolicy, out: &mut [Self]);
    fn pack_one(rgba: [f32; 4], mode: AlphaPolicy) -> Self;
}
```

`AlphaPolicy` (premul vs straight vs opaque) is a runtime parameter passed into `pack_*`, **not** a const on the trait. A `Rgba<f16>` value can be either straight or premul — premultiplication is a property of the conversion, not of the type. The critique flagged this and the critique is right.

**Concrete impls:**
- `DstPixel`: `Rgba<f16>`, `Rgba<f32>`, `[u8; 4]` (RGBA8), `[u16; 4]` (RGBA16).
- `SrcSlicePixel`: `Rgba<f16>` (premul-aware unpack — divides by alpha if non-zero), `Rgba<f32>`.
- `SrcReader`: implemented on **layout marker zero-sized types**, not on pixels. See dispatch below.

**Static dispatch from runtime layout (the critique's open question):**

The `BufferDesc` is a runtime value. The pipeline is a generic `<R: SrcReader, D: DstPixel>` function. The bridge is one `match` at the entry of `convert_row` / `convert_region` / `convert_buffer`:

```rust
fn convert_row<D: DstPixel>(&self, buf: &ImageBuffer, y: u32, dst: &mut [D], mode: AlphaPolicy) {
    use crate::convert::pipeline::*;
    match (buf.desc.planes.len(), buf.desc.planes[0].encoding) {
        (4, SampleFormat::U8)   => run::<RgbaU8Interleaved, D>(self, buf, y, dst, mode),
        (3, SampleFormat::U8)   => run::<RgbU8Interleaved,  D>(self, buf, y, dst, mode),
        (1, SampleFormat::U8)   => run::<GrayU8,            D>(self, buf, y, dst, mode),
        (2, SampleFormat::U8)   => run::<GrayAlphaU8,       D>(self, buf, y, dst, mode),
        (4, SampleFormat::U16Le|U16Be) => run::<RgbaU16Interleaved, D>(self, buf, y, dst, mode),
        // ...
        _ => run::<GenericReader, D>(self, buf, y, dst, mode), // scalar fallback path
    }
}
```

Each `run::<R, D>` is a separate monomorphization. Count = (# layouts handled) × (# dst pixel types). Today: ~6 readers × 4 dst = 24 specializations. Each is ~50 lines of inlined SIMD. Binary growth: small and bounded. The `GenericReader` catch-all uses the scalar `read_sample` path so any exotic layout still works, just slower. **No `matches(&BufferDesc) -> bool` runtime trait method** — the `match` does dispatch in one place.

**Planar vs interleaved fast path:** `RgbaU8Interleaved::read_x4` does `bytemuck::cast_slice::<u8, [u8; 4]>(row)[x..x+4]` then unpacks the four `[u8; 4]` into four lanes per channel — one indexed load + lane shuffle. A planar `RgbaU8Planar::read_x4` does four separate `&[u8]` reads (one per plane). Both fit the `SrcReader` signature; the implementations are very different. The trait does not force a one-size-fits-all read.

**Why:** today `simd.rs` has exactly two specializations. The trait split makes the algorithm reusable for every (layout, dst) pair, and the runtime→static `match` keeps the pipeline body monomorphic without exploding the API. The scalar `GenericReader` is the safety net for layouts the engine doesn't optimize.

**How to apply:**
1. Build the inner `run::<R, D>` function first — ~30 lines: full SIMD chunks, scalar remainder.
2. Add `RgbaU8Interleaved` (the most common layout) and `Rgba<f16>` `DstPixel` impl.
3. Wire it into `ColorConversion::convert_row`. The original `convert_buffer_row_to_acescg_simd` test in `simd.rs` becomes the regression bar — it must produce identical output.
4. Add other readers/dst pixels one at a time, each gated by a passing test.
5. Delete `acescg_f16_to_srgb_u8_simd` and `convert_buffer_row_to_acescg_simd` only after all callers are migrated.

---

### Step 4 — Delete `pack_rgba_premul` and the flat-`&mut [f32]` premultiply

**Where:** `src/convert/mod.rs`, `src/convert/premultiply.rs`, plus the three call sites of `pack_rgba_premul` (`buffer.rs:295`, `typed.rs:87`, `convert/mod.rs:13` itself).

**What:**
- `premultiply(&mut [f32], &ChannelLayoutKind)` and `unpremultiply(...)`: confirmed by grep, used only by their own tests. **Delete.**
- `pack_rgba_premul`: takes `([f32; 3], f32)`; is not a drop-in for `<Rgba<f16> as DstPixel>::pack_one([f32; 4], AlphaPolicy)`. The two call sites that survive Steps 5 and 7 deletions:
  - `buffer.rs:295` (`BandBuffer::extract_tile_rgba_f16`) — `BandBuffer` is deleted in Step 7. The call site disappears with it.
  - `typed.rs:87` (`TypedImage::read_region`) — `TypedImage` is deleted in Step 5. The call site disappears with it.
  After Steps 5 and 7, `pack_rgba_premul` has zero callers and can be deleted with its module.

**Why:** the duplication is real but the migration is **not** mechanical at the unit-of-call level — each caller is doing decode→matrix→pack inline and the pipeline replaces the whole inline sequence, not just the `pack_rgba_premul` step. The plan treats Step 4 as bookkeeping to be done after Steps 5/7, not before.

**How to apply:** keep `pack_rgba_premul` alive through Steps 5 and 7. Once those land, the function is unreferenced. Delete in Step 9 alongside `lib.rs` re-export cleanup.

---

### Step 5 — Collapse `TypedImage` into `ColorConversion`

**Where:** `src/image/typed.rs`, `src/image/mod.rs`, `src/lib.rs`.

**Verified:** workspace grep shows `TypedImage` is referenced only inside `typed.rs`, `mod.rs`, `lib.rs`. Zero external callers (UI, server, viewport, mip, tile). Safe to delete outright.

**What:** delete the `TypedImage` type and its three methods (`read_region`, `read_pixel`, `row_iter`). Replacements provided by Step 2:

| Old call | New call |
|----------|----------|
| `TypedImage::<Rgba<f16>>::from_raw(buf)?.read_region(0, 0, w, h)` | `conv.convert_region::<Rgba<f16>>(&buf, 0, 0, w, h, AlphaPolicy::PremultiplyOnPack)` |
| `tim.read_pixel(x, y)` | `conv.convert_region::<Rgba<f16>>(&buf, x, y, 1, 1, ...)` (rare; per-pixel API not needed) |
| `tim.row_iter()` | `(0..h).map(|y| conv.convert_region(&buf, 0, y, w, 1, ...))` |

`RawImage` stays — it is a type alias for `ImageBuffer` (verified in `image/raw.rs`), distinct purpose from `TypedImage`.

**Why:** `TypedImage` is `Arc<ImageBuffer> + ColorConversion + PhantomData<P>`. Pure wrapper, no behavior. The critique asked whether the viewport silently depends on it — verified: it does not. Phase 2 viewport pulls pixels through the tile store, not through `TypedImage`.

**How to apply:** delete the file `src/image/typed.rs`; delete the `mod typed;` and `pub use typed::TypedImage` lines in `src/image/mod.rs`; delete the `TypedImage` re-export in `src/lib.rs`. Move the existing tests' intent into Step 2's tests for `convert_region` (don't lose coverage).

---

### Step 6 — Rewrite `Tile<Rgba<f16>>::to_srgb_u8` and `to_f32_straight`

**Where:** `src/image/tile.rs`.

**Type discrepancy fix:** today `Tile<u8>` stores `Arc<Vec<u8>>` — flat bytes, 4 per pixel. `convert_pixels::<Rgba<f16>, [u8; 4]>` returns `Vec<[u8; 4]>`. Two options, pick (a):

(a) **Cast at the call site.** `Vec<[u8; 4]>` is bit-identical to `Vec<u8>` of length `4 * n` (no padding, `#[repr(transparent)]` array). Use `bytemuck::allocation::cast_vec::<[u8; 4], u8>(v)`. Zero copy. `Tile<u8>` keeps its existing API; nothing else changes.

(b) Change `Tile<u8>` → `Tile<[u8; 4]>` to keep types tight. Larger ripple: every consumer (frontend protocol writers, tests) updates. Not worth it for this refactor.

**What:**
```rust
impl Tile<Rgba<f16>> {
    pub fn to_srgb_u8(&self, conv: &ColorConversion) -> Tile<u8> {
        let pixels: Vec<[u8; 4]> = conv.convert_pixels::<Rgba<f16>, [u8; 4]>(
            &self.data, AlphaPolicy::Straight,
        );
        let bytes: Vec<u8> = bytemuck::allocation::cast_vec(pixels);
        Tile { coord: self.coord, data: Arc::new(bytes) }
    }

    pub fn to_f32_straight(&self) -> Vec<Rgba<f32>> {
        // Source is ACEScg, dst is also ACEScg (linear). Identity matrix, identity transfer.
        // Unpremul handled by SrcSlicePixel<Rgba<f16>>::unpack — divides by alpha if non-zero.
        let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::ACES_CG).unwrap();
        conv.convert_pixels::<Rgba<f16>, Rgba<f32>>(&self.data, AlphaPolicy::Straight)
    }
}
```

**Why:** the critique correctly flagged that `Vec<[u8; 4]>` ≠ `Vec<u8>`. `bytemuck::allocation::cast_vec` handles it without a copy because the inner array is `Pod` and unpadded. `to_f32_straight` was an orphan in v1 — it is now expressed with the same primitive, with an identity converter doing the alpha unpack work for free.

**How to apply:** drop `use crate::convert::simd::acescg_f16_to_srgb_u8_simd`. Add `bytemuck` to the use list (already a workspace dep). Update the existing tests in `tile.rs` — outputs must match bit-exact (see Step 9 regression harness).

---

### Step 7 — Drop the streaming pretense, delete `BandBuffer`

**Where:** `src/io/mod.rs`, `src/io/png.rs`, `src/io/tiff.rs`, `src/storage/source.rs`, `src/image/buffer.rs` (delete `BandBuffer`), new file `src/convert/tile_stream.rs`.

**Decision: Path A** (two-pass: `load` then `convert_to_tiles`). Path B (true streaming via `png::Reader::next_row`) is significant work and is justified only if profiling shows OOM. Defer.

**Trait surgery:**
```rust
pub trait ImageReader {
    fn can_handle(&self, path: &Path) -> bool;
    fn read_metadata(&self, path: &Path) -> Result<ImageMetadata, Error>;
    fn load(&self, path: &Path) -> Result<ImageBuffer, Error>;
}
```
`stream_to_tiles_sync` is removed from the trait and from PNG/TIFF impls.

**New module `src/convert/tile_stream.rs`** (not in `color/conversion.rs` — separation of concerns; the critique was right that `conversion.rs` should not host I/O orchestration):

```rust
pub fn convert_to_tiles(
    conv: &ColorConversion,
    src: &ImageBuffer,
    tile_size: u32,
    store: &TileStore,
    on_progress: Option<&(dyn Fn(u8) + Send)>,
) -> Result<(), Error>;
```

**Tile-direct write strategy (replaces the band-then-copy in v1):**

```rust
// For each band of tile_size rows:
//   For each tile column tx:
//     allocate tile_data: Vec<Rgba<f16>> of size tile_w * band_h
//     for local_y in 0..band_h:
//       conv.convert_row_strided(
//         src,
//         band_y + local_y,
//         x_start: tx * tile_size,
//         x_end:   tx * tile_size + tile_w,
//         dst:     &mut tile_data[local_y * tile_w .. (local_y + 1) * tile_w],
//         mode:    AlphaPolicy::PremultiplyOnPack,
//       );
//     store.write_tile_blocking(&Tile::new(coord, tile_data))?;
```

`convert_row_strided` is `convert_row` with explicit `[x_start, x_end)` instead of full row. Add it to `ColorConversion` in Step 2 (small extension; `convert_row` becomes `convert_row_strided(..., 0, w, ...)`).

**Parallelism:** the outer rayon parallelism stays. `(0..tiles_y_in_band).into_par_iter().for_each(|tx_chunk| { ... })` — parallel over tile **columns** within a band. This avoids nested rayon (the inner `convert_row_strided` is sequential per tile-column-stripe). Trade-off vs the v1 design (which parallelized rows): tile columns are coarser, cache-friendlier, and there is no scatter step.

**`BandBuffer` removal:** `BandBuffer::extract_tile_rgba_f16` (`buffer.rs:225..301`) used `pack_rgba_premul` + per-pixel `read_sample`. With `convert_to_tiles` writing tiles directly via `convert_row_strided`, `BandBuffer` has zero callers. Delete the type, the `pub use buffer::BandBuffer` re-export, the section header comment block, and any tests.

**Naming:** the module is `convert/tile_stream.rs` and the function is `convert_to_tiles` — the word "stream" is gone, matching the critique's point about the misleading name.

**How to apply:**
1. Add `convert_row_strided` to `ColorConversion`.
2. Create `convert/tile_stream.rs::convert_to_tiles` using the strategy above. Keep the existing `stream_image_buffer_to_tiles` in `io/mod.rs` alive in parallel until the migration is verified.
3. Update `FormatSource::stream_to_store` to call `convert_to_tiles`.
4. Update `PngFormat::stream_to_tiles_sync` and `TiffFormat::stream_to_tiles_sync` impls — but actually, just remove the trait method per "Trait surgery" above; `FormatSource` is the only caller.
5. Delete `stream_image_buffer_to_tiles` from `io/mod.rs`.
6. Delete `BandBuffer` from `image/buffer.rs` and its re-export from `image/mod.rs`.

---

### Step 8 — `TileStore` cleanup

**Where:** `src/storage/tile_store.rs`.

1. **`read_tile` clone fix:** line 127, line 149. Today returns `Tile::new(coord, cached.as_ref().clone())` and `Tile::new(coord, (*arc).clone())`. Change `Tile` so its `data` field is `Arc<Vec<P>>` (already is) and return `Tile { coord, data: Arc::clone(&arc) }`. No `Tile::new` wrapper for the cache hit path; that constructor should not own the clone semantics.
2. **`write_tile_blocking` clone fix:** line 158, `let arc = Arc::new(tile.data.clone())`. `tile.data` is already `Arc<Vec<...>>` — clone the Arc, not the Vec: `let arc = Arc::clone(&tile.data)`.
3. **`sample` benefits transitively:** since it goes through `read_tile`, fixing #1 fixes `sample` too. No separate change needed.
4. **`serialize_le` / `deserialize_le` fast path:** on LE hosts, single memcpy via `bytemuck::cast_slice::<Rgba<f16>, u8>` and the inverse. `Rgba<f16>` is already `Pod` (verified in `pixel/mod.rs`). Gate with `cfg(target_endian = "little")`. Keep per-pixel fallback for the BE branch.
5. **Delete legacy async API.** Verified by grep: `put`, `put_blocking`, `get`, `delete_tile`, `delete_tiles` have zero callers outside this file. **Delete unconditionally** (the v1 hedge "if production code uses them, leave them" is unjustified — the data shows no callers).

---

### Step 9 — Module reorg + golden regression test

**Final layout:**

```
src/convert/
    mod.rs           # tiny: pub use AlphaPolicy
    pipeline.rs      # SrcReader, SrcSlicePixel, DstPixel, run::<R,D>(), all impls.
                     # Replaces the deleted simd.rs and premultiply.rs.
    tile_stream.rs   # convert_to_tiles (was io/mod.rs::stream_image_buffer_to_tiles)

src/color/
    conversion.rs    # ColorConversion struct + convert_row/_strided/_region/_buffer/_pixels.
                     # Stays focused on colorimetry + the dispatch match. ~350 lines, not 500+.
                     # Inner pipeline body LIVES in convert/pipeline.rs::run<R,D>.
```

The critique pointed out that letting `conversion.rs` host pipeline + tile streaming would balloon it to 500+ lines mixing three concerns. Fix: `conversion.rs` keeps the public API and the runtime→static dispatch `match`, but delegates the inlined SIMD body to `convert/pipeline.rs::run<R, D>`. Tile streaming lives in its own module.

**`src/lib.rs` re-exports — final delta:**
- **Remove:** `TypedImage`, `convert::{premultiply, unpremultiply}`, `BandBuffer` (via `image::mod`), `ComponentEncoding` (renamed `SampleFormat`).
- **Add:** none — `ColorConversion` is already accessible via `pub mod color`.

**Golden bit-exact regression test (mandatory):**

Add `pixors-engine/tests/golden_conversion.rs`:

```rust
// Pseudo-outline; the implementer fills in the seed-driven harness.
#[test]
fn srgb_u8_to_acescg_f16_bitexact_against_v1() {
    // 1. Generate a deterministic 256x256 RGBA8 sRGB ImageBuffer (seeded RNG).
    // 2. Run the OLD pipeline (saved as a snapshot Vec<Rgba<f16>> committed under
    //    pixors-engine/tests/golden/srgb_u8_to_acescg.bin).
    // 3. Run the NEW pipeline: ColorConversion::convert_buffer.
    // 4. Assert byte-for-byte equality.
}
#[test]
fn acescg_f16_to_srgb_u8_bitexact_against_v1() { /* analogous */ }
#[test]
fn rgba16_be_tiff_to_acescg_bitexact_against_v1() { /* covers the endian change in Step 1 */ }
```

The golden binaries are produced **before** Step 1 starts (capture current output), committed to the repo, and then every subsequent step runs the test. Any drift fails CI immediately. This is the only credible defense against silent numerical regression and the v1 plan was wrong to omit it.

**Verification commands (run after each step):**
1. `cargo build --workspace`
2. `cargo test --workspace` (golden tests included)
3. `cargo clippy --workspace -- -D warnings`
4. Manual: open the desktop viewer + UI on a PNG and a TIFF; pan + zoom; visual diff against a screenshot taken before the refactor.

---

## 4. Out of scope (do not touch in this refactor)

- `src/color/matrix.rs`, `src/color/transfer.rs`, `src/color/primaries.rs`, `src/color/xyz.rs` — already clean.
- `src/image/mip.rs`, `src/image/tile.rs` (beyond Step 6) — works correctly.
- `src/server/**`, `src/storage/tile_cache.rs` — only adjust call sites that reference deleted names. No structural changes.
- `src/storage/source.rs` — only the `FormatSource::stream_to_store` body is touched (per Step 7). Trait + struct definition unchanged.
- `pixors-ui`, `pixors-viewport` — only fix references to deleted types/functions.

If you find yourself editing one of the above for reasons other than renamed call sites, stop and re-read this section.

---

## 5. Verification checklist

After every step:
1. `cargo build --workspace` — clean.
2. `cargo test --workspace` — all green. The `tests` modules in `simd.rs`, `conversion.rs`, `tile.rs`, `typed.rs` are the regression net for this refactor; port them to the new APIs, do not delete them.
3. `cargo clippy --workspace -- -D warnings` — clean.

Manual smoke test after Step 9:
4. Run the desktop viewer: `cd pixors-engine && cargo run -- example1.png`. Pan + zoom must look identical to before.
5. Run the UI dev server and load a PNG and a TIFF. Pixel-identical output expected (this refactor is a structural change, not a numerical one — bit-exact equality is the bar).

---

## 6. Order summary (one-pager)

| # | Step | Files touched | Net LOC |
|---|------|---------------|---------|
| 0 | Capture golden snapshots (PRE-refactor) | new `tests/golden/*.bin` + `tests/golden_conversion.rs` | +80 |
| 1 | `SampleFormat` + planar/interleaved typed-row helpers | `image/buffer.rs`, `io/tiff.rs`, `io/png.rs` | -50 |
| 2 | Universal `ColorConversion::convert_*` (+ dst field, + AlphaPolicy) | `color/conversion.rs` | +140 |
| 3 | `SrcReader` / `SrcSlicePixel` / `DstPixel` + `run<R,D>` | new `convert/pipeline.rs`, delete `convert/simd.rs` | -80 |
| 4 | Delete `pack_rgba_premul` + `premultiply.rs` (after Steps 5+7) | `convert/mod.rs`, `convert/premultiply.rs`, `lib.rs` | -150 |
| 5 | Delete `TypedImage` | `image/typed.rs`, `image/mod.rs`, `lib.rs` | -110 |
| 6 | `Tile::to_srgb_u8` + `to_f32_straight` via `convert_pixels` | `image/tile.rs` | -5 |
| 7 | Tile-direct write, delete `BandBuffer`, drop trait method | `io/mod.rs`, `io/png.rs`, `io/tiff.rs`, `storage/source.rs`, `image/buffer.rs`, new `convert/tile_stream.rs` | -180 |
| 8 | `TileStore` clone fixes + bytemuck + delete legacy async | `storage/tile_store.rs` | -120 |
| 9 | Module reorg + re-export trim | `lib.rs`, `convert/mod.rs`, `image/mod.rs` | -10 |

Expected net: ~500 fewer lines, one canonical conversion API, zero `convert_X_to_Y` named functions, golden tests gating every step.
