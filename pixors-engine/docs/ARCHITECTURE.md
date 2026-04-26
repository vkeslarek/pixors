# Pixors Architecture

> Authoritative reference for the pixors-engine architecture. Read top to bottom.
> Updated: Phase 8 compositor complete.

---

## 1. The Big Picture

pixors is an image editor where the **engine** (Rust library + server) does all heavy
lifting and the **frontend** (React + TypeScript) is a thin rendering client.

### Data flow from file open to pixel on screen

```
File on disk
  │
  ▼
IO Layer (src/io/)          ← decodes PNG/TIFF into layers
  │
  ▼
Convert Layer (src/convert/) ← converts ImageBuffer to ACEScg f16 tiles per layer
  │
  ▼
Storage Layer (src/storage/) ← TileStore (disk) + TileCache (RAM)
  │
  ▼
Composite Layer (src/composite/) ← blends N layer tiles → one display tile
  │
  ▼
Server Layer (src/server/)   ← serves display tiles via WebSocket
  │
  ▼
Frontend (pixors-ui/)       ← renders tiles on <canvas>
```

Every step is **tile-granularity**. No full-image buffer exists anywhere after IO.

---

## 2. Module Map

```
src/
  ── color/         Color spaces, transfer functions, matrices. Pure math.
  │   ├── conversion.rs   ColorSpace, ColorConversion (LUTs, convert_* API)
  │   ├── detect.rs       Chromaticity matcher, ICC classifier (shared PNG+TIFF)
  │   ├── matrix.rs       Matrix3x3 + Bradford adaptation
  │   ├── primaries.rs    RgbPrimaries, WhitePoint
  │   └── transfer.rs     TransferFn (sRGB gamma, Rec.709, etc.)
  │
  ── pixel/         Pixel types and pack/unpack.
  │   ├── component.rs    Component trait (u8, u16, f16, f32)
  │   ├── rgba.rs         Rgba<T> + Pixel impl
  │   ├── rgb.rs          Rgb<T> + Pixel impl
  │   ├── gray.rs         Gray<T>, GrayAlpha<T>
  │   ├── pack.rs         Pixel impl for [u8;3/4], [u16;3/4]
  │   ├── xyz.rs          CIE XYZ / xyY types
  │   └── format.rs       PixelFormat (Rgba8, Argb32 for WS protocol)
  │
  ── image/         Image data model. No pixel math.
  │   ├── document.rs     Image, Layer, LayerMetadata, Orientation, BlendMode
  │   ├── buffer.rs       BufferDesc, PlaneDesc, ImageBuffer, SampleFormat
  │   ├── tile.rs         TileCoord, TileGrid, Tile<P>
  │   ├── mip.rs          MipPyramid, generate_from_mip0
  │   └── meta.rs         AlphaMode, ChannelLayoutKind, SampleType
  │
  ── io/            File format decoders.
  │   ├── mod.rs          ImageReader trait (read_document_info, load_layer, load_document)
  │   ├── png.rs          PNG: EXPAND only (no STRIP_16), text metadata, pHYs, ICC
  │   └── tiff/
  │       ├── mod.rs      TIFF: 8/16/32-bit, YCbCr→RGB, CMYK/Lab refusal, multi-IFD
  │       ├── ycbcr.rs    YCbCr → RGB (BT.601)
  │       ├── cmyk.rs     CMYK refusal without ICC
  │       └── lab.rs      Lab refusal without ICC
  │
  ── convert/      Color conversion pipeline.
  │   ├── pipeline.rs     SrcReader trait + layout-specific impls, run::<R,D>(), SIMD helpers
  │   └── tile_stream.rs  convert_to_tiles: ImageBuffer → TileStore (ACEScg f16)
  │
  ── composite/    Tile compositor (stateless).
  │   └── mod.rs          composite_tile: blends N LayerView → one ACEScg f16 tile
  │
  ── storage/      Tile persistence and caching.
  │   ├── tile_store.rs   TileStore: disk-backed tiles with hot LRU cache
  │   ├── tile_cache.rs   TileCache: AcescgKey / DisplayKey LRU caches
  │   └── source.rs       FormatSource: async wrapper around ImageReader
  │
  ── server/       WebSocket server (Axum).
  │   ├── server.rs       Axum router, session management
  │   ├── session.rs      SessionManager (Arc<RwLock<Session>>)
  │   ├── service/
  │   │   ├── tab.rs      TabService: tab lifecycle, layer commands, image loading
  │   │   └── viewport.rs ViewportService: zoom/pan, tile streaming
  │   └── ws/             WebSocket frame encoding and dispatch
```

---

## 3. Key Abstractions

### 3.1 `Image` — what a file becomes

```
Image { layers: Vec<Layer>, metadata: ImageMetadata }
  └─ Layer { name, buffer: ImageBuffer, offset, opacity, visible, blend_mode }
       └─ ImageBuffer { desc: BufferDesc, data: Vec<u8> }
            └─ BufferDesc { planes: Vec<PlaneDesc>, color_space, alpha_mode }
                 └─ PlaneDesc { offset, stride, encoding: SampleFormat }
```

- **PNG** → 1 layer. **TIFF** → N layers (multi-page).
- Layers are **bottom-to-top** draw order.
- `BlendMode::Normal` only for now; extensible.

### 3.2 `Pixel` trait — the unified pack/unpack interface

```rust
pub trait Pixel: Copy + Pod {
    fn unpack(self) -> [f32; 4];                        // pixel → linear RGBA
    fn unpack_x4(s: &[Self]) -> (f32x4, f32x4, f32x4, f32x4);
    fn pack_x4(rr, gg, bb, aa, mode, out: &mut [Self]); // encoded RGBA → pixel
    fn pack_one(rgba, mode) -> Self;
}
```

Implemented for: `Rgba<f16>`, `Rgba<f32>`, `[u8; 3]`, `[u8; 4]`, `[u16; 3]`, `[u16; 4]`.

The conversion pipeline always works in `[f32; 4]` intermediate. `Pixel` is the **only** place where concrete types touch the pipeline — everything else is generic over `<D: Pixel>`.

### 3.3 `ColorConversion` — the only conversion entry point

```rust
conv.convert_row::<D>(buf, y, dst, AlphaPolicy)        // one row
conv.convert_row_strided::<D>(buf, y, x0, x1, dst, _)  // partial row
conv.convert_region::<D>(buf, x, y, w, h, _)            // rectangular region
conv.convert_buffer::<D>(buf, _)                         // full image (rayon)
conv.convert_pixels::<S, D>(&[S], _)                     // typed pixel slice
```

All five methods share the same SIMD inner loop (`pipeline.rs:run<R,D>`) via a
`match` dispatch on `(planes.len(), planes[0].encoding)` that picks the right
`SrcReader` ZST. No per-pixel match — static monomorphization.

### 3.4 `AlphaPolicy` — runtime premultiplication control

```rust
pub enum AlphaPolicy {
    PremultiplyOnPack,  // store (r*a, g*a, b*a, a) — ACEScg working format
    Straight,           // store (r, g, b, a) — sRGB display, operations
    OpaqueDrop,         // store (r*a, g*a, b*a) — RGB output, no alpha channel
}
```

This is a **runtime parameter**, not a trait const. A `Rgba<f16>` is neither
premul nor straight — the policy controls how it's packed.

### 3.5 `TileStore` + `TileCache` — the two-tier storage

```
                     TileStore (per layer, disk-backed)
                        │ hot cache: LRU 64 tiles
                        │ on-disk: /tmp/pixors/{tab}/layer_{n}/tile_{m}_{tx}_{ty}.raw
                        │
                     TileCache (global, RAM-only)
                        ├── acescg: AcescgKey → Arc<Vec<Rgba<f16>>>  (LRU 128)
                        └── display: DisplayKey → Arc<Vec<u8>>        (LRU 256)
```

- **TileStore** is the source of truth. Created per-layer at load time.
- **TileCache** is a cross-layer hot cache.
  - `AcescgKey { tab_id, layer_id, coord }` — layer-bound, survives opacity changes.
  - `DisplayKey { tab_id, coord, composition_sig }` — composite result, invalidates
    naturally when layer state changes via the signature hash.

### 3.6 Compositor — the blend math

Stateless. Single function:

```rust
pub fn composite_tile(req: &CompositeRequest<'_>) -> Result<Vec<Rgba<f16>>, Error>
```

Input: `&[LayerView]` (bottom-to-top) + `TileCoord` in composition space.
Output: one ACEScg f16 premultiplied tile.

Math (Porter-Duff over, premultiplied):

```
src_a = src.a * layer.opacity
out.rgb = src.rgb * opacity + dst.rgb * (1 - src_a)
out.a   = src_a            + dst.a   * (1 - src_a)
```

Unbounded ACEScg, no clamp. Clamp only at the final sRGB encode step.

Per-tile fetch helper (`fetch_overlapping_layer_tiles`) reads at most **4 tiles**
per layer per composite tile (for offset layers). Common case (offset 0,0, same
size) reads exactly 1 tile.

---

## 4. MIP Pyramid

Each `LayerSlot` has its own `MipPyramid` under `layer_{n}_mips/`.

```
layer_0_mips/
  ├── mip_0/ tile_0_0_0.raw ...   ← MIP-0 tiles (the source store at layer_{n}/)
  ├── mip_1/ tile_1_0_0.raw ...   ← 2×2 box-filter downscale via generate_from_mip0
  ├── mip_2/ ...
  └── ...

MIP selection: mip_level_for_zoom(zoom)
  zoom ≥ 0.5  → MIP 0 (full res)
  zoom ≥ 0.25 → MIP 1 (half)
  zoom ≥ 0.125 → MIP 2 (quarter)
  ...
```

MIP generation is **lazy**: `ensure_mip_level(zoom)` checks `generated` flag,
spawns `generate_from_mip0` via `tokio::spawn_blocking` only when needed.

When a MIP level is not yet generated, `layer_views_for_mip` falls back to the
layer's MIP-0 store with `mip_level = 0` — the compositor uses full-res tiles
until downscaled ones are ready. No visible empty frames.

---

## 5. Composition Signature (cache invalidation)

```
composition_sig = hash(visible_layers.id, offset, opacity, blend_mode)
```

Computed on every `get_tile_rgba8` call and embedded in `DisplayKey`.
When the user changes opacity/visible/offset/blend → `composition_sig` changes →
all `DisplayKey`s are now stale → cache miss → recomposite.

**No manual invalidation on layer state changes.** Old entries evict via LRU.

Pixel edits (Phase 6) invalidate per-layer `AcescgKey`s — this is explicit.

---

## 6. WebSocket Protocol

### Commands (client → server)

| Command | Fields | Effect |
|---------|--------|--------|
| `create_tab` | — | New tab |
| `close_tab` | `tab_id` | Close tab |
| `open_file` | `tab_id, path` | Load image |
| `viewport_update` | `tab_id, zoom, pan_x, pan_y, w, h` | Update viewport |
| `layer_set_visible` | `tab_id, layer_id, visible` | Toggle layer |
| `layer_set_opacity` | `tab_id, layer_id, opacity` | Set opacity |
| `layer_set_offset` | `tab_id, layer_id, x, y` | Move layer |

### Events (server → client)

| Event | Fields | Meaning |
|-------|--------|---------|
| `tab_created` | `tab_id, name` | New tab ready |
| `image_loaded` | `tab_id, width, height, layer_count` | Image decoded, tiles ready |
| `tiles_complete` | — | All visible tiles sent |
| `layer_changed` | `tab_id, layer_id, field, composition_sig` | Layer state mutated |
| `doc_size_changed` | `tab_id, width, height` | BBox changed (offset/visible) |
| `viewport_updated` | `tab_id, zoom, pan_x, pan_y` | Viewport acknowledged |

Binary tile messages: `[1-byte sid len, sid, 2-byte coord length, TileRect, pixels...]`

---

## 7. Performance Characteristics

| Stage | Bottleneck | Current approach |
|-------|-----------|------------------|
| IO decode | File read + decode | Full decode into `ImageBuffer` (RAM). Streaming deferred. |
| Color conversion | Per-pixel math | SIMD 4-wide via `f32x4` for U8/U16. `GenericReader` fallback for exotic. |
| MIP generation | 2×2 box filter | Rayon-parallel over tiles. One `spawn_blocking` per layer. |
| Composition | Per-pixel blend | Scalar f32 per pixel. `fetch_overlapping_layer_tiles` cached (1-4 reads). |
| WebSocket | Binary frames | Raw RGBA8 per tile, no compression. Tile-level granularity. |

Hot paths (color conversion, composite) have SIMD-ready structure but run scalar
until profiling demands otherwise.

---

## 8. Known Design Tensions

### 8.1 TileStore writes to disk even for ephemeral data

`convert_to_tiles` writes tiles to a `TileStore` (disk-backed). The MIP generator
reads them back from disk. The compositor reads from disk. Every stage round-trips
through disk even though a `TileStore` has a hot cache (LRU 64 tiles in RAM).

**Mitigation:** In practice, the hot cache keeps recently-accessed tiles in RAM.
The LRU is sized to hold working sets (64 tiles × 256² × 8 bytes ≈ 33 MB per
store). For single-layer images, the cache covers the full image. For multi-layer,
the cache thrashes if layers > 64 tiles × 4 overlapping tiles per composite.

**Future direction:** `TileSink` trait abstraction (see §9).

### 8.2 Session locking model

Tabs are removed from `Session`'s `HashMap`, mutated, and reinserted because
`RwLock<Session>` prevents holding a mutable reference while readers exist.
This works but is fragile — nothing prevents two tasks from mutating the same
tab concurrently (the remove-then-reinsert is not atomic across the two steps).

**Mitigation:** Single-threaded command dispatch per session (the WS handler
serializes commands). Concurrency is limited to `stream_tiles_for_tab` which
reinserts under the same lock.

**Future direction:** `Arc<RwLock<TabData>>` per tab, or a channel-based command
queue per tab.

### 8.3 `TabData` is a god object (838 lines)

`TabData` handles IO loading, tile management, MIP generation, composition
orchestration, and event emission. It has methods that belong to different
concerns (`open_image` = IO, `layer_views_for_mip` = storage, `get_tile_rgba8`
= composite dispatch).

**Future direction:** Split into `TabDocument` (image state) and `TabView`
(viewport/composite state). Or introduce `TileSink` to decouple load pipeline
from `TabData`.

---

## 9. Future: `TileSink` abstraction

Currently every tile stage is coupled to `TileStore` (disk). A `TileSink` trait
would allow:

```
TileSink<P> {
    fn accept(&self, coord: TileCoord, data: &[P]) -> Result<(), Error>;
}
```

With implementations:
- `TileStore` — write to disk with hot cache (current behavior)
- `MipSink` — accumulate 2×2 tiles → downsample → forward to another sink
- `CompositeSink` — blend N source sinks → forward to display sink
- `WsSink` — serialize and send via WebSocket (no disk)

This would let `convert_to_tiles` be generic over the sink, enabling pipelines
like:

```
PNG decode → Convert → [MIP gen chain] → Composite → WebSocket
                       (all in RAM, no disk)
```

Without breaking the current disk-backed path (`TileStore` implements `TileSink`).

---

## 10. Roadmap Alignment

| Phase | Status | What it delivered |
|-------|--------|-------------------|
| 1 | ✓ | Image I/O, PNG/TIFF decode, color conversion |
| 2 | ✓ | Viewport, swapchain, pan/zoom interactivity |
| 3 | ⏳ | `Operation` trait, brightness/contrast ops (CPU) |
| 4 | ⏳ | Tiled async engine, MIP pyramid |
| 5 | ⏳ | UI integration (React frontend) |
| 6 | ⏳ | Editor semantics: layers, masks, selections |
| 8 | ✓ | Multi-layer model, compositor, TIFF multi-page, per-layer stores |

Phase 8 was pulled forward so the compositor exists before Phase 6 (adjustment
layers need a compositor to work against).
