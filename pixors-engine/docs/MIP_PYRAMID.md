# MIP Pyramid

Multi-resolution pyramid for fast previews and progressive refinement. Each image carries one pyramid. Each MIP level is its own tile grid at uniform 256×256 tile size. The pyramid exists to make interactive editing feel instant while the full-resolution computation catches up.

## Summary (locked decisions)

| Aspect | Decision |
|---|---|
| Default generation filter | Box filter (average 2×2 → 1) |
| Higher-quality filters | Bilinear / Lanczos / Mitchell offered per-op or per-save |
| Canonical level | MIP 0 is the single source of truth after any edit |
| Higher MIP levels | Derived from MIP 0 via the generation filter |
| Minimum pyramid resolution | Stop generating below 64×64 |
| Operation MIP-awareness | **Every operation must be MIP-aware** — no opt-out |
| Invalidation | Edit to a MIP 0 tile invalidates all higher-MIP tiles it touches, cascading upward |
| Regeneration | Lazy — triggered by the scheduler when a MIP tile is needed |
| Preview-vs-canonical divergence | Accepted during transient refinement |

## Why a pyramid

- Viewport display often needs far fewer pixels than the full image (zoom-out, preview, thumbnail)
- Operations on a smaller level finish much faster — good enough to present **immediately**
- Progressive refinement fills in the correct result over time

The pyramid is a performance tool, not a storage requirement. A tile present at MIP 0 can always regenerate its higher-MIP counterparts.

## Level definition

For an image of size `(W, H)`:

- MIP 0: full resolution `(W, H)`
- MIP `n`: `(ceil(W / 2^n), ceil(H / 2^n))`
- Max level: smallest `N` such that `min(W, H) / 2^(N+1) < 64`, i.e. the highest level where both dimensions stay `≥ 64`

Formula: `mip_max = floor(log2(min(W, H) / 64))` (clamped at 0).

Examples:

| Image | MIP levels |
|---|---|
| 512×512 | 0..3 (4 levels, smallest = 64×64) |
| 1024×1024 | 0..4 (5 levels) |
| 4096×4096 | 0..6 (7 levels) |
| 8192×8192 | 0..7 (8 levels) |

Why 64×64: below that the image is essentially a thumbnail and further downsampling yields no useful signal. Storage cost of the full pyramid down to 64×64 is bounded: `Σ 1/4^n ≤ 4/3` of the MIP 0 cost.

## Tile grid per level

Every MIP level has its own tile grid, uniform 256×256 tile size. Tiles at the highest MIP levels are typically smaller than 256 in their valid region (see [TILE_SYSTEM D22](DECISIONS.md#d22--boundary-tiles-padded-to-full-size-valid-region-in-metadata)).

Total tiles across the full pyramid for 8K RGBA `f16`:
- MIP 0: 32×32 = 1024 tiles
- MIP 1: 16×16 = 256 tiles
- MIP 2..7: 64 + 16 + 4 + 1 + 1 + 1 = 87 tiles
- Total: 1367 tiles ≈ 683 MiB (vs 512 MiB for MIP 0 only)

## Generation filters

### Default: box filter

Average of a 2×2 block at MIP `n-1` → one pixel at MIP `n`. Cheap, trivially parallel, good enough for non-critical preview. Since internal data is already linear (ACEScg), the average is correct — no gamma correction step needed.

### Optional higher-quality filters

Offered when the user requests better final-output quality (e.g. export thumbnails, save with embedded MIPs):

- **Bilinear** — 2×2 with position-weighted average (cheap, small quality gain over box)
- **Lanczos** — 6-tap or 8-tap separable, high quality, highest cost
- **Mitchell–Netravali** — balanced, fewer ringing artifacts than Lanczos

Implemented as regular operations that take a source MIP level and write a target MIP level. The default pyramid-fill job uses box. A user can explicitly regenerate the pyramid with a different filter.

## MIP 0 is canonical

After any edit, **MIP 0 is the ground truth**. Every higher MIP level is derived from MIP 0 by the generation filter.

Consequences:

- A direct computation at MIP N (e.g. fast-preview blur) is temporary and expected to be replaced by the MIP-0-derived version
- Two observers of the same image must agree on MIP 0; they may transiently disagree on higher MIP levels during refinement
- Saving an image saves MIP 0 (plus embedded MIPs if the format asks for them, regenerated at save time)

## Every operation must be MIP-aware

An operation **must** produce a sensible result at any MIP level. This is a hard contract, not a capability flag.

### What MIP-aware means

- Given a kernel radius / parameter `R` expressed in MIP 0 pixels, the op applies the scaled parameter `R / 2^n` at MIP `n`
- Visual meaning stays consistent across MIPs: a "50 px blur" looks like a 50-px blur at any zoom
- Parameters that don't scale with resolution (e.g. color factors) stay unchanged

### Why this is required

A non-MIP-aware op breaks progressive refinement: the fast-preview at MIP N would look structurally different from the canonical MIP 0 result, causing a visible jump when refinement completes. Pixors rejects that experience.

If an operation cannot meaningfully scale to higher MIPs (e.g. exact-pixel morphology with fixed structuring element), its MIP N implementation must still produce a best-effort approximation that converges toward the correct result when the MIP 0 computation propagates up through the generation filter. In the worst case, the operation renders the viewport tiles at MIP 0 only and skips the fast-preview pass.

### Approximation is acceptable

A fast-preview computed directly at MIP N will often differ slightly from the MIP-0-derived result at MIP N. This is a feature, not a bug:

- Fast preview: immediate feedback at display resolution
- Canonical: correct result, progressively replaces the preview
- Brief visual transition during refinement is accepted as the price of responsiveness (and is still strictly better than waiting for full-res)

## Refinement flow

When the user applies an operation, the scheduler walks the following priority-ordered phases:

1. **Fast preview**
   Run the op on viewport-visible tiles at the currently-displayed MIP level. User sees the result immediately at display resolution.

2. **Canonical viewport**
   Run the op on viewport-visible tiles at MIP 0. This is the authoritative result for the region the user is looking at.

3. **Compose back up**
   Regenerate the invalidated higher-MIP tiles covering the viewport by filtering from MIP 0 upward. The display transitions from the fast-preview version to the canonical-derived version.

4. **Neighbor prefetch**
   Process tiles adjacent to the viewport at MIP 0, then compose their higher MIPs. Ready for pan.

5. **Background full coverage**
   Process remaining tiles across the whole image at MIP 0, then fill the full pyramid.

Each phase maps to a priority level in the [SCHEDULER](SCHEDULER.md) _(TBD)_.

## Invalidation

When a MIP 0 tile is modified:

1. Mark the tile dirty at MIP 0
2. For each higher MIP level `n = 1 .. mip_max`:
   - Compute the covering tile `(x >> n, y >> n)` at level `n`
   - Mark that tile **stale** (needs regeneration from MIP 0)
3. Stale tiles are regenerated lazily, driven by the scheduler (see phase 3 above)

A single MIP 0 tile propagates to at most one tile at each higher level (the grids halve). Total invalidation cost per edit is `O(mip_max)` tile marks.

## Tile identity across levels

Tiles at different MIP levels are distinct entities with distinct `TileId`s ([D23](DECISIONS.md#d23--tileid-is--image_id-mip_level-x-y--mip-level-is-part-of-identity)). The mapping between levels is:

```
MIP n tile (x, y) covers MIP 0 tile range:
    [x * 2^n,  (x+1) * 2^n) × [y * 2^n,  (y+1) * 2^n)
```

For example, MIP 3 tile `(2, 5)` covers the MIP 0 region of 8×8 tiles starting at `(16, 40)`.

## Storage

The pyramid is stored in the same tile store as MIP 0 (see [TILE_SYSTEM D25](DECISIONS.md#d25--per-image-tiles-stored-as-a-flat-vec-with-linear-offsets)). Per-MIP tile grids occupy adjacent ranges in the flat `Vec<Tile>` with per-MIP offset tables. No separate pyramid allocator.

Higher MIP tiles obey the same storage engine eviction rules as MIP 0 tiles. Pinning rules (viewport) apply to the currently-displayed MIP level and to MIP 0 during refinement.

## Save-time behavior

When saving to a format that supports embedded MIPs (TIFF, EXR with multi-part, KTX, some JPEG variants):

- Default: save MIP 0 only
- User can opt in to embedding MIPs, regenerated at save time with a chosen filter (often Lanczos for a final pass)

When saving to a format that does not support embedded MIPs (PNG, JPEG): the in-memory pyramid is discarded at save time except for MIP 0.

## Relations

- [TILE_SYSTEM](TILE_SYSTEM.md) — tile identity, boundary handling, work units
- [STORAGE_ENGINES](STORAGE_ENGINES.md) — where tiles live and how they move
- [OPERATIONS](OPERATIONS.md) — MIP-aware contract and parameter scaling _(TBD)_
- [SCHEDULER](SCHEDULER.md) — priority ordering for the refinement phases _(TBD)_
