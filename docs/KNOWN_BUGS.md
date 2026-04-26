# Known Bugs

## Multi-page TIFF Layer Flickering

**Severity:** Medium  
**Affects:** TIFF files with multiple layers (multi-page/multi-IFD)  
**Symptom:** Layers flicker on first load — they appear one by one as tiles arrive from their respective stream pipelines. After zooming, one layer becomes dominant (appears to "win") and others stop displaying.

**Observed behavior:**
- 6-layer TIFF (363×382, tile_size=256)
- Each layer has 4 tiles → stream pipeline emits per-layer
- `viewport_sink: stored 60 tiles, marking ready` fires 6× (once per layer)
- `stream_tiles: sending 4 tiles at mip=0 (desired=1)` — display MIP check fails for level 1
- `tile_fetching took 422ms` for 4 tiles (composite path is reading from disk, which may not be ready)
- 6× `generate_from_mip0` calls (one per layer), each generating 9 levels independently
- Only 1 `Background MIP 1 ready` event sent

**Root cause analysis:**
1. Each layer gets an independent stream pipeline (`ImageFileSource` → `ColorConvertPipe` → `MipPipe` → `tee` → sinks). Tiles from different layers arrive asynchronously.
2. The `Viewport` per layer stores tiles independently. The auto-stream callback (`vp_cb`) sends tiles to the frontend as they arrive per-layer, causing flickering.
3. `is_display_mip_ready` only checks the first tile at the desired MIP level — if ANY layer has it, it returns true. But for multi-layer compositing, ALL visible layers need their tiles.
4. When display cache misses, `get_tile_rgba8` falls through to `composite_tile` which reads from `WorkingWriter` (disk). If some layers' tiles haven't been flushed to disk yet, the composite produces partial results.
5. The slow `tile_fetching` (~400ms for 4 tiles) suggests composite path is running against layers that haven't fully loaded.

**Proposed fixes:**
- `is_display_mip_ready` should check ALL visible layers, not just any one
- Composite should wait for all visible layers, not degrade to partial results
- Consider a barrier/signal before sending first tiles to frontend for multi-layer images
- Or: composite all layers in the stream pipeline (via `CompositePipe`) instead of on-demand in `get_tile_rgba8`

**Logs:**
```
ImageLoaded tab=b6d7ffdb... 363x382 layers=6
source: emitted 4/4 tiles  (×24, all layers)
viewport_sink: stored 60 tiles, marking ready  (×6 layers)
stream_tiles: sending 4 tiles at mip=0 (desired=1)  — cache miss, fallback to MIP-0
tile_fetching took 422ms  — slow composite from disk
generate_from_mip0  (×6 layers, each 9 levels)
Background MIP 1 ready, notifying client  (only 1 event)
```
