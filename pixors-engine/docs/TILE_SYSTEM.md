# Tile System

How images decompose into tiles, how tiles are addressed, and how neighborhoods form work units.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Tile size | 256×256 pixels, uniform (see [D13](DECISIONS.md#d13--tile-size-256256)) |
| Boundary tiles | Padded to full 256×256; valid region tracked in metadata |
| `TileId` | Struct `{ image_id, mip_level, x, y }` |
| Dirty tracking | One dirty flag per tile (no sub-tile regions) |
| Per-image tile storage | 2D grid, `(x, y) ↔ linear offset` bijection — implementation detail |
| MIP identity | Each MIP level has its own tile grid; MIP level is part of `TileId` |

## Tile

```
Tile {
    id: TileId,
    storage_handles: per-engine Arc handles,   // presence indicates residency
    valid_region: (width, height),             // for boundary tiles, else (256, 256)
    dirty: bool,
    location_hint: StorageLocation,            // last known primary location
}
```

- Always a 256×256 buffer of RGBA `f16` interleaved
- `valid_region` marks the meaningful pixel area inside the buffer
- Padding (outside `valid_region`) holds zeros — consistent with premultiplied alpha (color * 0 = 0, alpha = 0)

## TileId

```
TileId {
    image_id: ImageId,
    mip_level: u8,      // 0 = full resolution
    x: u32,             // tile column at this MIP level
    y: u32,             // tile row at this MIP level
}
```

- `(x, y)` are **tile-grid coordinates at that MIP level**, not pixel coordinates
- MIP-distinct tiles are different entities. Tile `(image=42, mip=0, x=5, y=3)` and `(image=42, mip=1, x=5, y=3)` are separate tiles covering different image regions
- Struct layout is bijective with a linear offset if the engine wants to store tiles in a flat `Vec`: `offset = mip_offset[mip] + y * tiles_per_row[mip] + x`

## Boundary tiles

Image dimensions are rarely a multiple of 256. The last column / last row of tiles cover a partial region.

**Convention**: the tile buffer is **always 256×256**; unused area is zero-filled. `valid_region = (real_width_in_tile, real_height_in_tile)`.

For an image of `(W, H)` at MIP `m`:
- Scaled size at MIP `m`: `(W', H') = ((W + mask) >> m, (H + mask) >> m)` with appropriate rounding
- Tile grid dimensions: `(ceil(W' / 256), ceil(H' / 256))`
- Tile `(x, y)` valid region:
  - `width  = min(256, W' - x * 256)`
  - `height = min(256, H' - y * 256)`

Operations read the valid region; the padding is never authoritative. When an op writes to a boundary tile, it writes inside the valid region only (or writes the full buffer with same-zero padding if that's free).

Neighborhood reads past the image edge are handled by the op (common choices: clamp, mirror, zero). Boundary handling is a **per-op policy**, not a tile-level concern.

## Dirty tracking

One dirty flag per tile. If any pixel in the valid region has changed since the tile was last flushed to its authoritative storage, the flag is set.

Why not sub-tile regions:
- Most ops touch the whole tile anyway (per-pixel transforms, blur, color grade)
- Sub-tile dirty bitmaps add bookkeeping that rarely pays off
- Tile granularity matches transfer granularity ([D13f](DECISIONS.md#d18--batched-priority-sorted-transfer-submission))

When an op writes only a small sub-region of a tile (e.g. brush stroke), the **whole tile** is marked dirty and flushed as a whole. If this shows up as a real cost in interactive editing, revisit with targeted dirty regions for edit ops specifically.

## Per-image tile storage

An `Image` owns a **tile grid per MIP level**:

```
Image {
    id: ImageId,
    width: u32,
    height: u32,
    tile_size: u32 = 256,
    mip_count: u8,
    tiles: Vec<Tile>,          // flat storage, indexed by offset
    mip_offsets: Vec<usize>,   // [offset_mip_0, offset_mip_1, ...]
    mip_dims: Vec<(u32, u32)>, // [(tx_0, ty_0), (tx_1, ty_1), ...]
}
```

`(image_id, mip, x, y) → linear_offset` is computed as:

```
offset = image.mip_offsets[mip]
       + y * image.mip_dims[mip].0
       + x
```

Flat `Vec` is the baseline because most images are dense (non-sparse). If a gigapixel workflow shows up where large portions of the grid are empty, revisit with a sparse structure. Not doing that up front.

## Work units

A **work unit** is the smallest indivisible execution chunk handed to an op: one output tile plus whatever input neighbors are required.

Given an op with neighborhood `N = { left, right, top, bottom }`:

- Output tile: `(image, mip, x, y)`
- Input tiles: all `(image, mip, x', y')` with:
  - `x - left ≤ x' ≤ x + right`
  - `y - top  ≤ y' ≤ y + bottom`

Work unit formation happens in the scheduler. See [EXECUTION_MODEL](EXECUTION_MODEL.md) once written.

## Viewport-driven priority flow

The scheduler uses tile topology + viewport + current MIP level to drive priorities. The intended progressive refinement order:

1. **Viewport tiles at current displayed MIP level** — fastest preview
2. **Viewport tiles at MIP 0** — full resolution baseline
3. **MIP composition** — recompute higher MIP levels from updated MIP 0 tiles, gradually replacing the preview with the correct result
4. **Neighbor prefetch at MIP 0** — tiles adjacent to the viewport, ready for pan
5. **Background** — remaining tiles + full MIP pyramid for the whole image

This order belongs to the [SCHEDULER](SCHEDULER.md) and [MIP_PYRAMID](MIP_PYRAMID.md) docs; it is documented here so the tile design is visibly compatible with it.

## Coordinate conventions

- Pixel origin: top-left `(0, 0)`, Y grows downward
- Tile origin: top-left tile `(0, 0)` at any MIP level
- Tile `(x, y)` at MIP `m` covers pixel rectangle `[x*256, (x+1)*256) × [y*256, (y+1)*256)` in that MIP's image space
- To locate the same image region at two MIP levels, scale coordinates by `2^(m₂ - m₁)`

## Relations

- [DATA_MODEL](DATA_MODEL.md) — what lives inside a tile buffer
- [STORAGE_ENGINES](STORAGE_ENGINES.md) — how tile buffers are allocated, moved, evicted
- [MIP_PYRAMID](MIP_PYRAMID.md) — multi-resolution pyramid built out of these tiles _(TBD)_
- [EXECUTION_MODEL](EXECUTION_MODEL.md) — work unit formation, scheduling _(TBD)_
