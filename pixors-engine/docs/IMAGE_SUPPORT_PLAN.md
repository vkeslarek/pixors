# IMAGE_SUPPORT_PLAN — Full PNG + TIFF Support (with multi-layer foundation)

> Audience: an implementer model. This document is prescriptive. Goal is a clean, minimal architecture that decodes **every** PNG and TIFF the underlying crates (`png` 0.18, `tiff` 0.11) can decode, while introducing a **layer abstraction** so multi-page TIFF (and future PSD/EXR/OpenRaster) becomes a natural extension rather than a bolt-on.
>
> Architectural principle: **simpler beats clever**. Reuse the existing `BufferDesc`/`PlaneDesc` model. Only generalize where formats genuinely require it. Do not invent a new pixel taxonomy.

This revision incorporates a review pass; rationale for each non-obvious decision is inlined.

---

## 1. Current State (what works today)

Entry points: `src/io/png.rs`, `src/io/tiff.rs`, both implementing `ImageReader`.

### PNG — `src/io/png.rs`
Today it sets `Transformations::EXPAND | STRIP_16`, which:
- Expands palette → RGB, low-bit gray → 8-bit gray, expands `tRNS` to alpha.
- **Strips 16-bit down to 8-bit** — destructive. We lose precision for every 16-bit PNG.

Color-space detection (`detect_color_space`) is solid: cICP → iCCP (sRGB sniff only) → sRGB chunk → gAMA+cHRM → gAMA → fallback sRGB. The cHRM matcher is brittle (hard-coded primaries list, ±0.001 tolerance) but correct for the canonical cases.

Output `BufferDesc` is always 8-bit interleaved (`rgba8_interleaved`, `rgb8_interleaved`, `gray8_interleaved`, `gray_alpha8_interleaved`). No 16-bit path. No textual metadata captured (`tEXt`/`zTXt`/`iTXt`), no `pHYs` (physical resolution).

### TIFF — `src/io/tiff.rs`
Decodes via `decoder.read_image()` which materialises the **whole image** in memory as `DecodingResult::U8` or `U16`. Handles:
- 8-bit RGB / RGBA / Gray / GrayA
- 16-bit RGB / RGBA / Gray (no GrayA16)

Everything else returns `UnsupportedSampleType`:
- 1/4-bit palette, CMYK, YCbCr, Lab, separated, 32-bit int, 16/32-bit float, multi-sample (planar), multi-page (layers).
- Compression handled by `tiff` crate transparently — that part is fine.

Color-space detection: only checks `PhotometricInterpretation`. Ignores `ICCProfile` (tag 34675), gamma/chromaticity tags, `XResolution`/`YResolution`/`ResolutionUnit`, `Orientation`. Endianness of 16-bit data is host-native (`to_ne_bytes`); correct only because `tiff` crate already byte-swapped to host order during decode.

### Downstream consumers
The decoded `ImageBuffer` flows into `convert::tile_stream::convert_to_tiles`, which dispatches in `ColorConversion::convert_row_strided` based on `(planes.len(), planes[0].encoding)`. Specialised SIMD paths exist for **U8 (1/2/3/4 planes)** and **U16 RGBA**. Everything else falls through to `GenericReader` (per-sample dispatch via `SampleFormat::read_sample` — slow but correct).

This means: as soon as `BufferDesc` faithfully describes any layout, the engine already produces correct ACEScg f16 tiles. The slow path is acceptable for exotic formats; we add SIMD paths only when profiling demands it.

### Cargo features
- `png = "0.18"` — supports all standard chunks. We just need to read them.
- `tiff = "0.11"` — limited but sufficient for most baseline TIFF. Lacks: full 32-bit float in every layout, ICC tag exposure for some configurations, JPEG-in-TIFF, BigTIFF edge cases. For features `tiff` cannot decode, return a clean error and document the gap. Do **not** swap crates yet.

### Two enums you must not confuse
- **`SampleType`** (`src/image/meta.rs`): logical numeric type — `U8/U16/U32/F16/F32`. **No endianness.** Used as a high-level descriptor.
- **`SampleFormat`** (`src/image/buffer.rs`): byte-level encoding stored in `PlaneDesc::encoding` — `U8 / U16Le / U16Be / F16Le / F16Be / F32Le / F32Be`. **Has endianness.** Used by every read path that touches the raw byte buffer.

Throughout this plan the **canonical layout descriptor is `SampleFormat`**. `SampleType` exists for higher-level metadata only and never reaches the decoder. Any helper that builds a `BufferDesc` takes `SampleFormat` so the caller (PNG / TIFF) explicitly states the on-wire endianness.

---

## 2. Target Coverage Matrix

| Format | Sample types | Channel layouts | Color spaces | Layers |
|--------|--------------|-----------------|--------------|--------|
| **PNG** | 1/2/4/8/16-bit unsigned | Gray, GrayA, RGB, RGBA, palette (+tRNS) | sRGB, cICP, iCCP (full ICC bytes preserved + name sniff), gAMA+cHRM, gAMA only | Single (always 1 layer) |
| **TIFF** | u8, u16, u32, f16, f32 (LE/BE per file header) | Gray, GrayA, RGB, RGBA, palette (→RGB), YCbCr (→RGB) | PhotometricInterpretation + ICCProfile bytes + Gamma/Chromaticity tags | **Multi-page = multi-layer** |
| **TIFF CMYK / Lab** | any | any | **must have a recognised ICC profile** | as above |

Out of scope (future): PSD layers, OpenEXR multi-part, BigTIFF >4 GB tile streaming, JPEG-in-TIFF (until `tiff` crate or replacement supports it), animated PNG.

**Hard rule for CMYK / Lab**: without a recognised ICC profile we **refuse to decode** (`Error::UnsupportedColorSpace`) rather than ship visibly wrong colors via a naive formula. See §3.6.

---

## 3. Architectural Changes

### 3.1 New: `Document` / `Layer` model

A new top-level type representing **what was loaded from the file**:

```rust
// src/image/document.rs
pub struct ImageDocument {
    pub layers: Vec<Layer>,
    pub metadata: DocumentMetadata,
}

pub struct Layer {
    pub name: String,           // "Page 1", filename for PNG, PageName tag for TIFF
    pub buffer: ImageBuffer,    // existing type — unchanged
    pub offset: (i32, i32),     // top-left in document coordinates
    pub opacity: f32,           // 1.0 default
    pub visible: bool,          // true default
    pub orientation: Orientation, // EXIF-style; Identity default
}
```

- For PNG: always exactly **one layer** named after the file stem.
- For TIFF: one layer per IFD (page). Layer offsets come from `XPosition`/`YPosition` tags when present, else `(0, 0)`. `orientation` from tag 274.
- **No `BlendMode` yet.** YAGNI: the compositor does not exist. Add the field when the compositor lands. `opacity` and `visible` stay because they are trivial scalars and unblock a layer-visibility toggle in the UI.

```rust
// EXIF-style orientation. Engine never auto-rotates pixels at decode time;
// the field rides along so the renderer / a future "apply orientation" op
// can act on it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default] Identity,        // 1
    FlipH,                      // 2
    Rotate180,                  // 3
    FlipV,                      // 4
    Transpose,                  // 5
    Rotate90,                   // 6
    Transverse,                 // 7
    Rotate270,                  // 8
}
```

```rust
#[derive(Default, Debug, Clone)]
pub struct DocumentMetadata {
    pub source_format: Option<String>,            // "PNG", "TIFF"
    pub source_path:   Option<std::path::PathBuf>,
    pub dpi:           Option<(f32, f32)>,        // physical resolution
    pub text:          std::collections::HashMap<String, String>,
                                                   // PNG tEXt/zTXt/iTXt; TIFF
                                                   // ImageDescription / Software /
                                                   // Artist / Copyright / DateTime
    pub raw_icc:       Option<Vec<u8>>,           // verbatim bytes for a future CMM
}
```

`raw_icc` holds the original ICC bytes even when we cannot fully interpret them, so a future colour-management engine never has to re-open the file (addresses review point 8).

### 3.2 `ImageReader` trait — single migration step

Replace the current trait in **one** step (no temporary wrappers, no dead code period):

```rust
pub trait ImageReader: Send + Sync {
    fn can_handle(&self, path: &Path) -> bool;

    /// Number of layers and document-level metadata (no pixel decode).
    fn read_document_info(&self, path: &Path) -> Result<DocumentInfo, Error>;

    /// Per-layer metadata (dims, color space, alpha mode) — no pixel decode.
    fn read_layer_metadata(&self, path: &Path, layer: usize)
        -> Result<LayerMetadata, Error>;

    /// Decode one layer in full.
    fn load_layer(&self, path: &Path, layer: usize) -> Result<Layer, Error>;

    /// Convenience: decode the whole document.
    fn load_document(&self, path: &Path) -> Result<ImageDocument, Error> {
        let info = self.read_document_info(path)?;
        let layers = (0..info.layer_count)
            .map(|i| self.load_layer(path, i))
            .collect::<Result<_, _>>()?;
        Ok(ImageDocument { layers, metadata: info.metadata })
    }
}

pub struct DocumentInfo {
    pub layer_count: usize,
    pub metadata: DocumentMetadata,
}

pub struct LayerMetadata {
    /// Holds dims, planes, color_space, alpha_mode in one shot.
    pub desc: BufferDesc,
    pub orientation: Orientation,
    pub offset: (i32, i32),
    pub name: String,
}
```

`LayerMetadata` embeds `BufferDesc` directly (review point 9): `BufferDesc` already carries `width`, `height`, `color_space`, `alpha_mode` and the per-plane layout. Duplicating those scalar fields would invite drift.

The trait migration includes `FormatSource` and `storage::source` in the **same commit** (review point 3). `storage::source::FormatSource::open` becomes `open_layer(path, layer)` with `open(path)` as a `pub fn` shorthand for `open_layer(path, 0)`. No grace period, no temporary `load`/`read_metadata` shims.

### 3.3 `SampleFormat` extensions

Current enum (`src/image/buffer.rs`): `U8, U16Le, U16Be, F16Le, F16Be, F32Le, F32Be`.

Add only what TIFF actually needs:
```rust
pub enum SampleFormat {
    // existing ...
    U32Le, U32Be,
}
```

**No sub-byte variants.** PNG `EXPAND` already widens 1/2/4-bit gray and palette to 8-bit at the decoder boundary, where it belongs.

`PlaneDesc::read_sample` gains `U32Le`/`U32Be` arms that divide by `u32::MAX as f32`.

### 3.4 New `BufferDesc` constructors

Each format calls the constructor matching its on-wire layout directly — no `buffer_desc_for` indirection (review points 18, 19). The factories below extend the existing pattern.

```rust
impl BufferDesc {
    // 16-bit gaps (host-native — used by callers that already byte-swapped, e.g. TIFF crate output)
    pub fn gray_alpha16_interleaved(...) -> Self;

    // Explicit-endian 16-bit (used by PNG, which leaves data big-endian on the wire)
    pub fn rgb16be_interleaved(...)        -> Self;
    pub fn rgba16be_interleaved(...)       -> Self;
    pub fn gray16be_interleaved(...)       -> Self;
    pub fn gray_alpha16be_interleaved(...) -> Self;

    // f16 family (host-native; TIFF crate emits host-order)
    pub fn gray_f16_interleaved(...)       -> Self;
    pub fn gray_alpha_f16_interleaved(...) -> Self;
    pub fn rgb_f16_interleaved(...)        -> Self;
    pub fn rgba_f16_interleaved(...)       -> Self;

    // f32 family (host-native)
    pub fn gray_f32_interleaved(...)       -> Self;
    pub fn gray_alpha_f32_interleaved(...) -> Self;
    pub fn rgb_f32_interleaved(...)        -> Self;
    pub fn rgba_f32_interleaved(...)       -> Self;

    // u32 family (host-native)
    pub fn gray32_interleaved(...)  -> Self;
    pub fn rgb32_interleaved(...)   -> Self;
    pub fn rgba32_interleaved(...)  -> Self;
}
```

A single private `interleaved(...)` already exists; reuse it. CMYK has **no** factory — the CMYK byte buffer never escapes `src/io/tiff/cmyk.rs` (see §3.6).

PNG 16-bit caller passes `*16be_*` factories explicitly (review point 2): the `png` crate yields 16-bit samples in big-endian byte order regardless of host. The host-native `*16_interleaved` factories stay as-is for TIFF, since `tiff::DecodingResult::U16(Vec<u16>)` is already host-order.

### 3.5 Color-space detection — shared module

Both PNG and TIFF need: ICC sniffing, chromaticity matching, gamma classification. Move to `src/color/detect.rs`:

```rust
pub fn match_chromaticities(
    white: (f32, f32),
    red:   (f32, f32),
    green: (f32, f32),
    blue:  (f32, f32),
    tol: f32,
) -> Option<(RgbPrimaries, WhitePoint)>;

pub struct IccClassification {
    pub color_space: Option<ColorSpace>,  // None = unknown but still usable
    pub raw:         Vec<u8>,             // always preserved for raw_icc
}

/// Robust ICC sniffer: parses the standard 128-byte header + optional
/// `desc` tag. Returns a known ColorSpace only when ALL of the following
/// match a built-in entry:
///   - profile_class  (header byte 12..16) == "mntr"  (display)
///   - color_space    (header byte 16..20) == "RGB "  (CMYK / Lab handled separately)
///   - desc tag string (normalised: lowercased, hyphens/dots/underscores collapsed)
///     matches one of: "srgb iec61966 2 1", "adobe rgb 1998", "display p3",
///     "prophoto rgb", "rec2020", "dci p3 d65".
/// Robustness: hyphens vs dots vs underscores in profile names are normalised
/// before comparison (review point 6).
pub fn classify_icc_profile(bytes: &[u8]) -> IccClassification;

/// Same idea for CMYK / Lab profiles. Returns the source profile identifier
/// (e.g. "swop_v2", "fogra39") so the caller can pick a built-in conversion
/// path; None means "unknown profile, refuse to decode".
pub fn classify_icc_profile_cmyk(bytes: &[u8]) -> Option<CmykProfileId>;
pub fn classify_icc_profile_lab (bytes: &[u8]) -> Option<LabProfileId>;
```

`CmykProfileId` / `LabProfileId` are small enums of the profiles we ship pre-baked LUTs for (initially empty — we may ship none and always refuse CMYK in v1). Either way, the API hook is in place.

### 3.6 CMYK, Lab, and YCbCr → RGB

Live under `src/io/tiff/` because they are TIFF-specific (review point 20):

- `src/io/tiff/ycbcr.rs` — BT.601 matrix (or `YCbCrCoefficients` tag override). Always safe; YCbCr in TIFF is a transport encoding, not a colorimetric statement. Outputs RGB(A) in the photometrically-detected RGB color space (sRGB by default).
- `src/io/tiff/cmyk.rs` — **only** invoked when `classify_icc_profile_cmyk` returns a known profile we have a baked LUT for. Otherwise:
  ```rust
  return Err(Error::UnsupportedColorSpace(format!(
      "CMYK TIFF without a recognised ICC profile cannot be decoded \
       accurately; embed a known CMYK profile (e.g. SWOP, Fogra) or \
       convert to RGB before loading"
  )));
  ```
  Rationale (review point 7): the naive `R = (1-C)(1-K)` formula produces colors that are visibly wrong for any real CMYK source. Refusing is honest; silently lying is not. We can ship a few pre-baked CMYK→sRGB LUTs (SWOP v2, Fogra39) when needed; the dispatch point already exists.
- `src/io/tiff/lab.rs` — same policy as CMYK. Lab → XYZ → RGB is colorimetrically defined, so we *can* implement it without an ICC profile, but the source D-illuminant assumption is profile-dependent. v1: refuse without ICC; revisit when a fixture demands it.

After conversion the buffer is plain RGB(A) and goes through the standard `BufferDesc::rgb*` / `rgba*` factories.

### 3.7 PNG 16-bit path

Drop `Transformations::STRIP_16`. Keep `EXPAND` (palette → RGB, `tRNS` → alpha, low-bit gray → 8-bit gray). The `png` crate then yields native 16-bit data in the output buffer.

The crate writes 16-bit values as **big-endian byte pairs** in the output buffer (per the PNG spec). Use the `*16be_*` factories from §3.4 directly. Add a regression test: 16-bit RGBA PNG round-trips through `convert_buffer` to f16 with maximum per-channel error < `2/65535`.

### 3.8 PNG `sBIT` and `bKGD`

`sBIT`: log at debug level if it indicates the image uses fewer bits than the container.
`bKGD`: ignored at decode (compositor concern).

### 3.9 PNG ICC profile — full preservation

Today: any non-`sRGB` profile is silently treated as sRGB. Replace with `classify_icc_profile`:
- Always store the raw bytes in `DocumentMetadata::raw_icc`.
- If the classifier returns a known `ColorSpace`, use it.
- Otherwise fall back to sRGB **and** log a warn-level message including the profile description, so the user understands why colors may be off until a CMM is wired.

### 3.10 PNG `tEXt` / `zTXt` / `iTXt` and `pHYs`

- Iterate textual chunks via `png::Info::uncompressed_latin1_text` / `compressed_latin1_text` / `utf8_text`. Insert each `(keyword, value)` into `DocumentMetadata::text`.
- `pHYs`: `png::Info::pixel_dims` gives x/y pixels-per-unit and unit (Meter / Unspecified). When unit == Meter, convert to DPI (`px/m × 0.0254`) and store in `DocumentMetadata::dpi`.

### 3.11 PNG `acTL`/`fcTL` (animated PNG)

Out of scope. Decode the default frame and log a warning.

### 3.12 TIFF endianness, planar config, tiles vs strips, compression

The `tiff` crate handles all of these inside `read_image()`. **Do not reimplement.** What we surface:

- **Planar configuration** (`PlanarConfiguration` tag): `read_image()` returns chunky-form data for both modes; existing interleaved `BufferDesc` works.
- **BigTIFF**: handled by the crate.
- **Compression**: handled by the crate.

The only TIFF tag-walking we add is for color-space detection (§3.5), document metadata (§3.13), and multi-IFD iteration (§3.14).

### 3.13 TIFF document metadata

Read once during `read_document_info`:
- `XResolution` / `YResolution` / `ResolutionUnit` (296) → `DocumentMetadata::dpi`. Unit 2 = inch → values are DPI directly; unit 3 = cm → multiply by 2.54.
- `ImageDescription` (270), `Software` (305), `Artist` (315), `Copyright` (33432), `DateTime` (306) → `DocumentMetadata::text`.
- `ICCProfile` (34675) → `DocumentMetadata::raw_icc` and feed through `classify_icc_profile`.

Per-layer (read inside `decode_current_ifd`):
- `PageName` (285) → `Layer::name`.
- `XPosition` (286), `YPosition` (287) → `Layer::offset` (convert from inches/cm to pixels using the layer's own resolution).
- `Orientation` (274) → `Layer::orientation` (mapped to the `Orientation` enum). We **do not** rotate pixels at decode time; the field rides along.

### 3.14 TIFF multi-page (the "layers" angle)

`tiff::Decoder` exposes `next_image() -> TiffResult<()>` and `more_images() -> bool`. Loop:

```rust
fn load_document(&self, path: &Path) -> Result<ImageDocument, Error> {
    let mut decoder = open_tiff(path)?;
    let metadata    = read_document_metadata(&mut decoder, path)?;
    let mut layers  = Vec::new();
    let mut idx     = 0;
    loop {
        layers.push(decode_current_ifd(&mut decoder, idx, &metadata)?);
        if !decoder.more_images() { break; }
        decoder.next_image().map_err(|e| Error::Tiff(e.to_string()))?;
        idx += 1;
    }
    Ok(ImageDocument { layers, metadata })
}
```

`load_layer(path, n)` opens a fresh decoder and steps `n` times — simple, no shared state. For documents with hundreds of pages this becomes O(n²); add a cached `MultiPageReader` only if profiling shows it matters.

### 3.15 `PixelFormat` and the WS protocol

`src/pixel/format.rs::PixelFormat` (`Rgba8`, `Argb32`) is part of the WebSocket protocol — what the **renderer** ships to the frontend. It is not part of the loader path. **No change** for this plan: layers and high-bit-depth buffers live entirely engine-side; the WS protocol still ships 8-bit sRGB per tile. When the UI grows multi-layer awareness, that is a separate WS-protocol change (a new `tab` / `layer` message), not a `PixelFormat` extension.

This plan does not touch `PixelFormat`.

---

## 4. Reusable Abstractions

These are the **only** new abstractions introduced. Everything else reuses existing types.

### 4.1 `src/color/detect.rs`
Shared chromaticity matcher, ICC classifier (RGB / CMYK / Lab variants), gamma → `TransferFn`. See §3.5. Pure functions, no I/O. **Real value**: the same logic is needed by every format that can carry a profile.

### 4.2 `src/io/tiff/{cmyk,ycbcr,lab}.rs`
Stateless decoders operating on `&[u8]` / `&[u16]` slices. Live next to the TIFF reader because they are TIFF-specific decode-time concerns. No PNG path uses them.

### 4.3 `LayerMetadata` / `DocumentMetadata` / `Layer` / `ImageDocument`
In `src/image/document.rs`, re-exported from `src/image/mod.rs`. `LayerMetadata` embeds `BufferDesc` rather than duplicating its fields.

### 4.4 What we deliberately do NOT add
- **No `decode_helpers::buffer_desc_for`.** Each format already has a `match` over its native color-type enum; a wrapper would just relocate the match. Direct factory calls are clearer.
- **No `src/io/decode_helpers.rs` module.** Without `buffer_desc_for` it would be empty.

---

## 5. Error Surface

Add to `src/error.rs`:
```rust
#[error("layer index {index} out of bounds (document has {count} layers)")]
LayerIndexOutOfBounds { index: usize, count: usize },

#[error("ICC profile parsing failed: {0}")]
IccProfile(String),
```

Existing `UnsupportedSampleType` / `UnsupportedColorSpace` cover the rest. CMYK / Lab without recognised ICC use `UnsupportedColorSpace` with an actionable message.

---

## 6. Migration Path (recommended order)

Each step is independently testable and leaves the tree green.

1. **Add `ImageDocument` / `Layer` / `LayerMetadata` / `DocumentMetadata` / `Orientation`** in `src/image/document.rs`. Re-export. Tests: trivial constructors.
2. **Extend `BufferDesc`** with the missing factories from §3.4 (host-native 16-bit GA, all f16/f32, u32, explicit-BE 16-bit). Pure additions, no caller changes.
3. **Add `U32Le` / `U32Be`** to `SampleFormat`. Update `PlaneDesc::read_sample`.
4. **Move shared color detection into `src/color/detect.rs`.** Refactor `png.rs::detect_color_space` to use it. No behavior change. Tests: chromaticity matcher; ICC sniffer with hyphen/dot/underscore variants of the canonical names; ICC sniffer rejects non-`mntr` / non-`RGB ` headers.
5. **PNG: drop `STRIP_16`.** Wire the 16-bit path with `*16be_*` factories. Add fixture + test.
6. **PNG: read `tEXt`/`zTXt`/`iTXt` and `pHYs`** into `DocumentMetadata`. Plumb `raw_icc` through.
7. **TIFF: add f16, f32, u32 sample paths.** Each is a one-line `DecodingResult` arm.
8. **TIFF: YCbCr converter.** Always-safe path.
9. **TIFF: ICC profile reading + CMYK/Lab refusal.** Tag 34675 → `raw_icc` + `classify_icc_profile`. CMYK / Lab without recognised profile → `UnsupportedColorSpace` with actionable message.
10. **TIFF: read DPI / textual tags / orientation / page name / page offset.**
11. **Migrate the trait + `FormatSource` together** (review points 3, 10): introduce `read_document_info` / `read_layer_metadata` / `load_layer` / `load_document`, delete `read_metadata` / `load`, update `FormatSource::open` → `open_layer`, update every caller in `storage::source` and `server::service::tab`. One commit, no dead-code grace period.
12. **TIFF: multi-IFD iteration.** Multi-page TIFFs now load as multi-layer documents.

Steps 1–10 are pure additions. Step 11 is the only breaking change — done in a single coordinated commit. Step 12 unlocks multi-layer documents.

---

## 7. Performance Considerations

- **Do not premature-optimize SIMD readers.** `GenericReader` in `pipeline.rs` already handles every new layout correctly via per-sample dispatch. Add `SrcReader` impls (e.g. `RgbaF16Interleaved`) only when a real workload measures too slow. The dispatch in `convert_row_strided` falls through to `GenericReader` cleanly.
- The biggest perf foot-gun is loading the **entire** TIFF into RAM (current `decoder.read_image()`). For multi-GB TIFFs we want strip/tile streaming. Out of scope for this plan; flag as a follow-up. Mark in code: `// TODO: streaming decode for large TIFFs`.

---

## 8. Testing Strategy

Place fixtures in `pixors-engine/tests/fixtures/{png,tiff}/`. Keep them tiny (≤16 px) to commit safely.

### PNG fixtures
- 1-bit gray, 2-bit gray, 4-bit gray, 8-bit gray, 16-bit gray
- 8-bit GrayA, 16-bit GrayA
- 8-bit RGB, 16-bit RGB
- 8-bit RGBA, 16-bit RGBA
- Palette without tRNS, palette with tRNS
- sRGB chunk, gAMA-only, gAMA+cHRM (Adobe RGB primaries), iCCP (sRGB profile), iCCP (Display P3), cICP (P3)
- One PNG with `tEXt` + `pHYs` (assert metadata round-trip)

### TIFF fixtures
- Per sample type (u8, u16, u32, f16, f32) × per layout (Gray, GrayA, RGB, RGBA)
- YCbCr 8 (4:4:4 only — `tiff` crate limit)
- Palette
- 2-page and 3-page multi-page TIFF (layer test)
- Big-endian and little-endian variants of one type
- One TIFF with `XResolution` / `Orientation` / `PageName` (assert metadata round-trip)
- One CMYK TIFF without ICC → assert `UnsupportedColorSpace` (refusal is the contract)

### Test files
- `tests/png_decode.rs` — one test per PNG fixture: load, assert dims/color-space/alpha-mode, sample 1–3 pixels through `convert_pixels` to sRGB f32 and compare with expected RGB ±1e-3.
- `tests/png_metadata.rs` — assert text / DPI / raw_icc round-trip.
- `tests/tiff_decode.rs` — same shape as PNG decode tests.
- `tests/tiff_metadata.rs` — DPI / orientation / page name.
- `tests/tiff_multilayer.rs` — load multi-page document, assert `layers.len()` and per-layer names.
- `tests/tiff_cmyk_refused.rs` — assert refusal contract.
- `tests/color_detect.rs` — chromaticity matcher; ICC name normalisation (`"sRGB IEC61966-2.1"` and `"sRGB IEC61966-2-1"` and `"sRGB_IEC61966_2_1"` all classify to sRGB); ICC rejection of non-`mntr` / non-`RGB ` headers.
- `tests/alpha_policy_opaque_drop.rs` — exercises `AlphaPolicy::OpaqueDrop` for the new RGB (3-channel) source paths (review point 16). Existing tests only cover `PremultiplyOnPack` and `Straight`.

Every test must pass before merging the corresponding migration step.

---

## 9. Code Skeleton (for reference)

```rust
// src/image/document.rs  (NEW)
use crate::image::buffer::BufferDesc;
use crate::image::ImageBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default] Identity,
    FlipH, Rotate180, FlipV, Transpose, Rotate90, Transverse, Rotate270,
}

pub struct Layer {
    pub name: String,
    pub buffer: ImageBuffer,
    pub offset: (i32, i32),
    pub opacity: f32,
    pub visible: bool,
    pub orientation: Orientation,
}

impl Layer {
    pub fn from_buffer(name: impl Into<String>, buffer: ImageBuffer) -> Self {
        Self {
            name: name.into(), buffer,
            offset: (0, 0), opacity: 1.0,
            visible: true, orientation: Orientation::Identity,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct DocumentMetadata {
    pub source_format: Option<String>,
    pub source_path:   Option<std::path::PathBuf>,
    pub dpi:           Option<(f32, f32)>,
    pub text:          std::collections::HashMap<String, String>,
    pub raw_icc:       Option<Vec<u8>>,
}

pub struct LayerMetadata {
    pub desc: BufferDesc,
    pub orientation: Orientation,
    pub offset: (i32, i32),
    pub name: String,
}

pub struct DocumentInfo {
    pub layer_count: usize,
    pub metadata: DocumentMetadata,
}

pub struct ImageDocument {
    pub layers: Vec<Layer>,
    pub metadata: DocumentMetadata,
}

impl ImageDocument {
    pub fn single_layer(name: impl Into<String>, buffer: ImageBuffer) -> Self {
        Self {
            layers: vec![Layer::from_buffer(name, buffer)],
            metadata: DocumentMetadata::default(),
        }
    }
}
```

```rust
// src/io/tiff.rs  (sketch)
fn decode_current_ifd(
    dec: &mut Decoder<...>,
    idx: usize,
    doc_meta: &DocumentMetadata,
) -> Result<Layer, Error> {
    let (w, h)      = dec.dimensions()?;
    let color_type  = dec.colortype()?;
    let color_space = detect_color_space(dec, doc_meta);
    let alpha_mode  = alpha_mode_for(color_type);
    let raw         = dec.read_image()?;

    let buffer = match (color_type, raw) {
        (ColorType::RGB(8),   DecodingResult::U8(d))  => buffer_rgb8(w, h, d, color_space, alpha_mode),
        (ColorType::RGBA(8),  DecodingResult::U8(d))  => buffer_rgba8(w, h, d, color_space, alpha_mode),
        (ColorType::CMYK(8),  DecodingResult::U8(d))  => cmyk::to_rgb8_via_icc(w, h, d, doc_meta)?,
        (ColorType::YCbCr(8), DecodingResult::U8(d))  => ycbcr::to_rgb8(w, h, d, color_space, dec)?,
        (ColorType::RGB(16),  DecodingResult::U16(d)) => buffer_rgb16(w, h, d, color_space, alpha_mode),
        // ... GrayA16, f16, f32, u32 arms ...
        (ct, _) => return Err(Error::unsupported_sample_type(format!(
            "TIFF color type {:?} not yet supported", ct))),
    };

    Ok(Layer {
        name: read_page_name(dec).unwrap_or_else(|| format!("Page {}", idx + 1)),
        buffer,
        offset: read_page_offset(dec).unwrap_or((0, 0)),
        opacity: 1.0,
        visible: true,
        orientation: read_orientation(dec).unwrap_or_default(),
    })
}
```

---

## 10. What we explicitly do NOT do (keep the architecture simple)

- **No new `Pixel` impls** until SIMD readers actually need them. The `GenericReader` path already handles every layout via the `BufferDesc` abstraction.
- **No CMYK in the working color model.** We convert at decode time, refuse without recognised ICC profile. Layers stay in 4-channel RGBA all the way through the engine.
- **No `BlendMode` field on `Layer`.** Add when the compositor exists.
- **No streaming TIFF decoder.** Future work; `TODO` in code.
- **No PSD / OpenEXR / animated PNG.** The `Layer`/`Document` types are designed so adding them later is "implement `ImageReader`" — no engine surgery.
- **No ICC color-management engine.** `classify_icc_profile` only recognises known profile descriptors. `raw_icc` is preserved on every load so a future CMM never has to re-open the file.
- **No sub-byte `SampleFormat` variants.** PNG `EXPAND` handles 1/2/4-bit at the decoder boundary.
- **No `buffer_desc_for` indirection module.** Each format calls factories directly.
- **No `PixelFormat` changes.** WS protocol stays 8-bit sRGB until a separate UI plan addresses multi-layer / high-bit-depth display.

---

## 11. Definition of Done

- `cargo test -p pixors-engine` green, including all new fixtures and the `OpaqueDrop` policy test.
- `cargo clippy --workspace -- -D warnings` clean.
- `examples/load_any.rs` (new) opens any file in `tests/fixtures/` and prints `ImageDocument` info: layer count, names, per-layer color space, document metadata.
- Loading a 2-page TIFF returns `ImageDocument { layers.len() == 2 }`.
- 16-bit PNGs round-trip through `convert_buffer::<Rgba<f16>>` with maximum per-channel error < `2/65535`.
- CMYK TIFF without recognised ICC returns `Error::UnsupportedColorSpace` with an actionable message — never silently wrong colors.
- Documentation updated: `docs/DECISIONS.md` gains a new D-entry referencing this plan; `CLAUDE.md` "Phase Summary" mentions the layer abstraction is in place ahead of Phase 6.
