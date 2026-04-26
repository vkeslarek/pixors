# Pixors Architecture

> Authoritative reference for the pixors-engine architecture. Updated: Phase 8 stream pipeline.

---

## 1. The Big Picture

pixors is an image editor where the **engine** (Rust library + server) does all heavy
lifting and the **frontend** (React + TypeScript) is a thin rendering client.

### Data flow from file open to pixel on screen

```
File on disk
  │
  ▼
TileSource (src/stream/source.rs)    ← decodes PNG/TIFF into raw tile frames
  │
  ▼
ColorConvertPipe (src/stream/color.rs) ← source → sRGB u8, 4ch (display) or f16 (disk)
  │
  ▼
MipPipe (src/stream/mip.rs)          ← recursive 2×2 box-filter: emits MIP-0..MIP-N
  │
  ▼
tee() (src/stream/pipe.rs)           ← fans out to 3 sinks
  │
  ├── ViewportSink → Viewport        ← RAM cache (u8 sRGB), auto-streams to WS
  ├── WorkingSink → WorkingWriter    ← disk (f16 ACEScg), all MIP levels
  └── ProgressSink → frontend         ← percent-complete events
```

Every step is **tile-granularity**, running on dedicated threads with `mpsc::sync_channel(64)` for
backpressure. No full-image buffer exists anywhere after decode.

---

## 2. Module Map

```
src/
  ── stream/        Streaming tile pipeline (central data-flow architecture)
  │   ├── frame.rs       Frame, FrameMeta, FrameKind (Cow<[u8]>)
  │   ├── pipe.rs        Pipe trait, tee() (fan-out)
  │   ├── source.rs      TileSource trait, ImageFileSource, WorkSource
  │   ├── color.rs       ColorConvertPipe (source→sRGB u8 or f16 ACEScg)
  │   ├── mip.rs         MipPipe (recursive 2×2 box-filter downsampler)
  │   ├── sink.rs        TileSink trait, Viewport (RAM cache), ViewportSink, WorkingSink
  │   ├── composite.rs   CompositePipe (placeholder for multi-layer blending)
  │   ├── progress.rs    ProgressSink (percent-complete callback)
  │   └── mod.rs
  │
  ── color/         Color spaces, transfer functions, matrices. Pure math.
  │   ├── conversion.rs   ColorSpace (primaries + white point + transfer)
  │   ├── transfer.rs     TransferFn enum (9 functions: sRGB, Rec.709, PQ, HLG...)
  │   ├── primaries.rs    RgbPrimaries, WhitePoint
  │   ├── chromaticity.rs Chromaticity (CIE xy)
  │   ├── detect.rs       match_chromaticities(), IccClassification
  │   ├── pipeline.rs     SrcReader trait, run<R,D>(), SIMD helpers
  │   └── mod.rs
  │
  ── convert/      Color conversion engine.
  │   ├── conversion.rs   ColorConversion (LUT-based, SIMD, 5 convert_* APIs)
  │   ├── matrix.rs       Matrix3x3 (SIMD mul_vec_simd_x4), rgb_to_xyz, Bradford CAT
  │   ├── pipeline.rs     SrcReader trait + layout impls (works with ImageBuffer)
  │   ├── tile_stream.rs  convert_to_tiles (full-image to tiles, legacy)
  │   └── mod.rs
  │
  ── pixel/         Pixel types and pack/unpack.
  │   ├── component.rs    Component trait (u8, u16, f16, f32)
  │   ├── rgba.rs         Rgba<T> + Pixel impl (f16 unpacks to straight linear)
  │   ├── rgb.rs          Rgb<T> + Pixel impl
  │   ├── gray.rs         Gray<T>, GrayAlpha<T>
  │   ├── pack.rs         Pixel impl for [u8;3/4], [u16;3/4]
  │   ├── xyz.rs          CIE XYZ / xyY types
  │   ├── format.rs       PixelFormat (Rgba8, Argb32 for WS protocol)
  │   └── mod.rs          AlphaPolicy enum (Straight, PremultiplyOnPack, OpaqueDrop)
  │
  ── image/         Image data model. No pixel math.
  │   ├── document/
  │   │   ├── mod.rs      Image, ImageMetadata, ImageInfo
  │   │   └── layer.rs    Layer, LayerMetadata, Orientation, BlendMode
  │   ├── buffer.rs       BufferDesc, PlaneDesc, ImageBuffer, SampleFormat
  │   ├── tile.rs         TileCoord, TileGrid, Tile<P>
  │   ├── mip.rs          MipLevel, MipPyramid, generate_from_mip0
  │   ├── meta.rs         AlphaMode, ChannelLayoutKind, SampleType
  │   └── mod.rs
  │
  ── storage/      Tile persistence (disk I/O only).
  │   ├── writer.rs       TileWriter<P> trait, WorkingWriter (f16 disk store)
  │   └── mod.rs
  │
  ── composite/    Tile compositor (stateless).
  │   └── mod.rs          composite_tile(): Porter-Duff over blend, ACEScg f16
  │
  ── io/            File format decoders.
  │   ├── mod.rs          ImageReader trait
  │   ├── png.rs          PNG: EXPAND only, text/ICC/pHYs metadata
  │   └── tiff/mod.rs     TIFF: 8/16/32-bit, YCbCr/CMYK/Lab refusal, multi-IFD
  │
  ── server/       WebSocket server (Axum).
  │   ├── app.rs          AppState (composition root)
  │   ├── event_bus.rs    EngineCommand/EngineEvent untagged enums
  │   ├── session.rs      SessionManager (lazy create, TTL-based expiry)
  │   ├── server.rs       Axum router, WS upgrade
  │   ├── service/
  │   │   ├── tab.rs      TabService: tab lifecycle, layer commands, open_image pipeline
  │   │   └── viewport.rs ViewportService: zoom/pan, tile streaming, MIP selection
  │   └── ws/             WebSocket frame encoding, reader/writer loops
```

---

## 3. The Stream Pipeline

The stream pipeline is the central data-flow architecture. Tiles flow through a chain of
`Pipe` transforms running in dedicated threads, connected by `mpsc::sync_channel(64)`.

### 3.1 Core Types

```rust
// The unit of data flowing through pipes
pub struct Frame {
    pub meta: FrameMeta,       // layer_id, mip_level, image_w, image_h, color_space
    pub kind: FrameKind,       // Tile { coord } | Progress | LayerDone | StreamDone
    pub data: Cow<'static, [u8]>, // raw pixel bytes
}

// A transform: takes a receiver, returns a receiver
pub trait Pipe: Send + 'static {
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame>;
}

// A terminal consumer: runs in its own thread
pub trait TileSink: Send + Sync + 'static {
    fn run(&self, rx: mpsc::Receiver<Frame>) -> JoinHandle<()>;
}
```

### 3.2 Pipeline in open_image

```rust
// 1. Open source stream
let rx = ImageFileSource::open(path, tile_size)?;

// 2. Convert to sRGB u8 (display), using source image's plane count (RGB=3, RGBA=4)
let rx = ColorConvertPipe::new(
    src_cs, SRGB, AlphaPolicy::Straight, output_f16=false, src_desc
).pipe(rx);

// 3. Generate MIP pyramid (recursive 2×2 box filter)
let rx = MipPipe::new(tile_size, max_levels).pipe(rx);

// 4. Fan out to 3 sinks
let [vp_rx, wk_rx, prog_rx] = tee(rx, 3);

// 5a. Viewport: RAM cache, auto-streams tiles to frontend via callback
let _vp = ViewportSink::new(viewport).run(vp_rx);

// 5b. Working: sRGB u8 → f16 ACEScg premul → disk (all MIP levels)
let wk_rx = ColorConvertPipe::new(SRGB, ACES_CG, PremultiplyOnPack, output_f16=true, RGBA_desc)
    .pipe(wk_rx);
let wk = WorkingSink::new(store).run(wk_rx);

// 5c. Progress: emits percent-complete events to frontend
let _prog = ProgressSink::new(pct_cb).run(prog_rx);

// Return immediately — tiles stream in background
// disk_handle joined in LayerSlot::Drop
self.layers.push(LayerSlot { ..., disk_handle: Some(wk) });
```

### 3.3 ColorConvertPipe

Converts pixel data from source to destination color space. Supports two output formats:
- **u8 mode** (`output_f16=false`): `Vec<[u8;4]>` — 4 bytes/pixel, for Viewport display
- **f16 mode** (`output_f16=true`): `Vec<Rgba<f16>>` — 8 bytes/pixel, for disk storage

Uses the source image's `BufferDesc` to determine plane count (RGB=3, RGBA=4, Gray=1) and
builds per-tile descriptors with correct stride/offset.

### 3.4 MipPipe

Recursive downsampling pipe. Receives MIP-0 tiles, accumulates 2×2 blocks keyed by
`(src_mip, dst_tx, dst_ty)`, box-filters when complete, and re-enters generated tiles
into the same loop — enabling a single pipe to produce all MIP levels without chaining.

Handles edge tiles correctly via `actual_w`/`actual_h` computation. Pass-through is
guaranteed even for degenerate tiles.

### 3.5 Viewport

RAM tile cache shared between the stream pipeline and the server's tile-serving code.

```rust
pub struct Viewport {
    tiles: RwLock<HashMap<(u32, TileCoord), Arc<Vec<u8>>>>, // (mip_level, coord) → RGBA u8
    ready: AtomicBool,                                        // set on StreamDone
    pub on_tile_added: Option<Arc<dyn Fn(u32, TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
}
```

The `on_tile_added` callback auto-streams tiles to the WebSocket frontend as they
arrive — no request/response roundtrip needed for the initial load.

### 3.6 WorkingWriter

Disk-backed ACEScg f16 tile store. Pure I/O — no color conversion.

```
{base_dir}/tile_{mip_level}_{tx}_{ty}.raw
```

Methods: `read_tile()`, `write_tile_f16()`, `sample(x,y)`, `has()`, `destroy()`.
Auto-destroys on drop (configurable via `auto_destroy` flag).
Endian-aware serialization via `bytemuck`.

---

## 4. Color Science

### 4.1 ColorSpace

```rust
pub struct ColorSpace { primaries: RgbPrimaries, white_point: WhitePoint, transfer: TransferFn }
```

Predefined constants: `SRGB`, `LINEAR_SRGB`, `REC709`, `REC2020`, `ADOBE_RGB`,
`DISPLAY_P3`, `DCI_P3`, `PROPHOTO`, `ACES2065_1`, `ACES_CG`.

`matrix_to(dst) → Matrix3x3`: compute the linear-RGB→linear-RGB matrix.
`converter_to(dst) → ColorConversion`: build a full converter with LUTs.

### 4.2 ColorConversion

Precomputed converter between two color spaces. Owns decode_u8 LUT (256 entries) and
encode LUT (4096 entries). Five public APIs share the same SIMD inner loop:

```rust
conv.convert_row::<D>(buf, y, dst, alpha)            // one row
conv.convert_row_strided::<D>(buf, y, x0, x1, dst)    // partial row
conv.convert_region::<D>(buf, x, y, w, h, alpha)       // rectangular region
conv.convert_buffer::<D>(buf, alpha)                    // full image (rayon for h>256)
conv.convert_pixels::<S, D>(&[S], alpha)                // typed pixel slice
```

### 4.3 AlphaPolicy

```rust
pub enum AlphaPolicy {
    PremultiplyOnPack,  // store (r*a, g*a, b*a, a) — ACEScg working format
    Straight,           // store (r, g, b, a) — sRGB display
    OpaqueDrop,         // store (r*a, g*a, b*a) — RGB, no alpha channel
}
```

Runtime parameter, not a trait const. A `Rgba<f16>` is neither premul nor straight —
the policy controls how it's packed.

### 4.4 Matrix3x3

Column-major 3×3 matrix with SIMD `mul_vec_simd_x4` using `wide::f32x4`. Also provides
`rgb_to_xyz_matrix()`, `bradford_cat()` (chromatic adaptation), and `rgb_to_rgb_transform()`
(full source→destination matrix with optional CAT).

### 4.5 Image I/O

**`ImageReader` trait** (io/mod.rs):
- `can_handle(path) → bool`
- `read_document_info(path) → ImageInfo`
- `read_layer_metadata(path, layer) → LayerMetadata`
- `stream_tiles(path, tile_size, writer, layer, on_progress) → Result<()>`

**PNG** (io/png.rs): Row-by-row streaming via `RowAccumulator`. EXPAND transform only
(no STRIP_16). Supports sRGB/gAMA/cHRM/iCCP/cICP metadata.

**TIFF** (io/tiff/mod.rs): 8/16/32-bit. YCbCr/CMYK/Lab stubs (return errors).
Multi-IFD for multi-page support.

---

## 5. Compositor

Stateless tile-level over-blend in ACEScg f16 premultiplied:

```rust
pub fn composite_tile(req: &CompositeRequest<'_>) -> Result<Vec<Rgba<f16>>, Error>
```

Input: `&[LayerView]` (bottom-to-top) + `TileCoord`. Output: one ACEScg f16 premul tile.

Math (Porter-Duff over, premultiplied):
```
src_a = src.a * layer.opacity
out.rgb = src.rgb          + dst.rgb * (1 - src_a)
out.a   = src.a * opacity  + dst.a   * (1 - src_a)
```

`LayerView` references a `WorkingWriter` for reading source tiles. Fetches at most 4
tiles per layer per composite tile (for offset layers).

---

## 6. MIP Pyramid

Each `LayerSlot` has a `MipPyramid` under `layer_{n}_mips/`.

```
layer_0_mips/
  ├── mip_0/ tile_0_0_0.raw ...   ← MIP-0 tiles (source store at layer_{n}/)
  ├── mip_1/ tile_1_0_0.raw ...   ← 2×2 box-filter via generate_from_mip0
  └── ...
```

**Display MIPs** (stream pipeline): Generated eagerly by `MipPipe` during `open_image`,
stored in `Viewport` RAM cache. Primary source for tile streaming.

**Storage MIPs** (disk): Generated lazily by `generate_from_mip0` → rayon-parallel over
tiles. Used as fallback when display cache misses.

MIP selection: `level_for_zoom(zoom)`:
- zoom ≥ 0.5 → MIP 0
- zoom ≥ 0.25 → MIP 1
- zoom ≥ 0.125 → MIP 2

---

## 7. WebSocket Protocol

### Commands (client → server)

| Command | Fields | Effect |
|---------|--------|--------|
| `create_tab` | — | New tab |
| `close_tab` | `tab_id` | Close tab |
| `activate_tab` | `tab_id` | Switch to tab |
| `open_file` | `tab_id, path` | Load image |
| `viewport_update` | `tab_id, zoom, pan_x, pan_y, w, h` | Update viewport |
| `request_tiles` | `tab_id, x, y, w, h, zoom` | Request visible tiles |
| `layer_set_visible` | `tab_id, layer_id, visible` | Toggle layer |
| `layer_set_opacity` | `tab_id, layer_id, opacity` | Set opacity |
| `layer_set_offset` | `tab_id, layer_id, x, y` | Move layer |

### Events (server → client)

| Event | Fields | Meaning |
|-------|--------|---------|
| `tab_created` | `tab_id, name` | New tab ready |
| `image_loaded` | `tab_id, width, height, layer_count` | Image metadata ready |
| `image_load_progress` | `tab_id, percent: u8` | Streaming progress (0-100) |
| `tiles_complete` | — | All visible tiles sent |
| `mip_level_ready` | `tab_id, level, width, height` | Background MIP generation done |
| `layer_changed` | `tab_id, layer_id, field, comp_sig` | Layer state mutated |
| `doc_size_changed` | `tab_id, width, height` | BBox changed |
| `viewport_updated` | `tab_id, zoom, pan_x, pan_y` | Viewport acknowledged |

### Binary tile messages

36-byte header (little-endian) + RGBA8 pixel data:
```
[4B px][4B py][4B width][4B height][4B mip_level][16B tab_id UUID][pixels...]
```

---

## 8. Performance Characteristics

| Stage | Bottleneck | Approach |
|-------|-----------|----------|
| IO decode | PNG zlib streaming | Single-threaded (limitation of PNG format). Tiles emitted as decoded. |
| Color conversion | Per-pixel math | SIMD 4-wide via `f32x4`. Precomputed LUTs (256+4096 entries). Rayon for large tiles. |
| MIP generation | 2×2 box filter | Recursive single-pipe. Downsample runs in accumulator thread. |
| Composition | Per-pixel blend | Scalar f32. 1-4 tile reads per layer per composite. |
| Disk writes | f16 serialization + fs::write | Background thread, all MIP levels in parallel. |
| WebSocket | Binary frames | Tile-level raw RGBA8, no compression. Auto-stream on first load. |

Stream pipeline uses `mpsc::sync_channel(64)` for backpressure — prevents OOM on
large images by bounding buffer between stages to 64 frames.

---

## 9. Known Design Tensions

### 9.1 Display MIPs vs Storage MIPs

Two parallel MIP pyramids exist: display (sRGB u8, RAM, via `MipPipe` in stream) and
storage (ACEScg f16, disk, via `generate_from_mip0` lazily). When `ensure_mip_level`
runs for the first zoom-out, it regenerates MIPs from disk that were already computed
during the stream pipeline. Future: unify into a single pipeline with two branches (§4.6
in PHASE_8_REVIEW.md).

### 9.2 TileCoord hash key sensitivity

`Viewport` stores tiles keyed by `(mip_level, TileCoord)`. `TileCoord` has 7 fields
including `px, py, width, height`. If the MipPipe produces a tile with `width=256`
but the client requests it with `width=232` (edge tile), the lookup fails. The current
MipPipe computes correct edge dimensions via `actual_w`/`actual_h` — if this regresses,
cache misses occur silently.

### 9.3 Session locking model

Tabs are removed from `Session`'s `HashMap`, mutated, and reinserted because
`RwLock<Session>` prevents holding a mutable reference while readers exist.

### 9.4 No cancellation

Pipes don't listen to cancellation tokens. If `close_image` is called mid-load, threads
continue until `StreamDone`. `JoinHandle` in `LayerSlot::Drop` blocks the caller.

---

## 10. Deleted Types (Legacy Architecture)

| Old Type | Replacement | Where |
|----------|-------------|-------|
| `DisplayWriter` | `stream::Viewport` | RAM cache for display tiles |
| `FanoutWriter` | `stream::tee()` | Fan-out to multiple consumers |
| `TileStore` | `storage::WorkingWriter` | Disk-backed tile storage |
| `TileCache` (global) | `stream::Viewport` (per-layer) | Cross-layer LRU — never implemented |
| `OpenContext` (`image/open.rs`) | Stream pipeline in `tab.rs::open_image` | Image opening orchestration |
| `generate_display_mips_blocking` | `stream::MipPipe` | Display MIP generation |
| `color_space_from_params()` | `ColorSpace::with_optional_params()` | Color vector construction |
| `transfer_from_gamma()` | `TransferFn::from_gamma()` | Gamma value → transfer function |

---

## 11. Roadmap Alignment

| Phase | Status | What it delivered |
|-------|--------|-------------------|
| 1-2 | ✓ | Image I/O, color conversion, viewport, pan/zoom |
| 3 | ⏳ | `Operation` trait, brightness/contrast ops |
| 4 | ⏳ | Tiled async engine, MIP pyramid |
| 5 | ⏳ | UI integration (React frontend) |
| 6 | ⏳ | Editor semantics: layers, masks, selections |
| 8 | ✓ | Stream pipeline, multi-layer compositor, TIFF multi-page, per-layer stores |
