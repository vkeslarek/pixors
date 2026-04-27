//! Tile-level layer compositor.
//!
//! Stateless, single-tile over-blend in ACEScg f16 premultiplied.
//! No full-image buffer is ever allocated — one tile at a time.
//!
//! All math is the Porter-Duff "src over dst", premultiplied form:
//!   out.rgb = src.rgb + dst.rgb * (1 - src.a)
//!   out.a   = src.a   + dst.a   * (1 - src.a)
//! Layer opacity is applied as a pre-scale to the source pixel
//! before blending.

use crate::error::Error;
use crate::image::{BlendMode, TileCoord};
use crate::pixel::Rgba;
use crate::storage::WorkingWriter;
use half::f16;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct LayerView<'a> {
    pub id:        Uuid,
    pub store:     &'a WorkingWriter,
    pub size:      (u32, u32),
    pub offset:    (i32, i32),
    pub opacity:   f32,
    pub blend:     BlendMode,
    pub mip_level: u32,
}

pub struct CompositeRequest<'a> {
    pub layers:    &'a [LayerView<'a>],
    pub coord:     TileCoord,
    pub tile_size: u32,                 // needed for layer-tile division math
}

// ---------------------------------------------------------------------------
// Composite entry point
// ---------------------------------------------------------------------------

pub fn composite_tile(req: &CompositeRequest<'_>) -> Result<Vec<Rgba<f16>>, Error> {
    let n = req.coord.pixel_count();
    let mut out = vec![Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO); n];

    for layer in req.layers {
        let intersect = intersect_tile_layer(req.coord, req.tile_size, layer);
        let Some((ox, oy, ow, oh, lx0, ly0)) = intersect else { continue };

        // Fetch at most 4 layer-local tiles that cover the intersection
        let tiles = fetch_overlapping_layer_tiles(layer, req.tile_size, lx0, ly0, ow, oh)?;
        if tiles.is_empty() { continue; }

        // Blend each pixel
        for dy in 0..oh {
            for dx in 0..ow {
                let layer_x = lx0 + dx;
                let layer_y = ly0 + dy;

                let tx = layer_x / req.tile_size;
                let ty = layer_y / req.tile_size;
                let lx = layer_x % req.tile_size;
                let ly = layer_y % req.tile_size;

                // Find the right tile
                let tile_data = tiles.iter().find(|(c, _)| c.tx == tx && c.ty == ty);
                let Some((coord, data)) = tile_data else { continue };

                let src_idx = (ly * coord.width + lx) as usize;
                if src_idx >= data.len() { continue; }
                let src = data[src_idx];

                let out_idx = ((oy + dy) * req.coord.width + ox + dx) as usize;
                let dst = out[out_idx];

                let src_f = to_rgba_f32(src);
                let dst_f = to_rgba_f32(dst);

                let blended = match layer.blend {
                    BlendMode::Normal => over_blend(src_f, dst_f, layer.opacity),
                };

                out[out_idx] = from_rgba_f32(blended);
            }
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Tile fetch helper
// ---------------------------------------------------------------------------

/// Fetch at most 4 layer-local tiles covering the rectangle
/// `(lx0, ly0, w, h)` in layer-local pixel coordinates.
fn fetch_overlapping_layer_tiles(
    layer: &LayerView<'_>,
    tile_size: u32,
    lx0: u32,
    ly0: u32,
    w: u32,
    h: u32,
) -> Result<Vec<(TileCoord, Arc<Vec<Rgba<f16>>>)>, Error> {
    let tx0 = lx0 / tile_size;
    let ty0 = ly0 / tile_size;
    let tx1 = (lx0 + w - 1) / tile_size;
    let ty1 = (ly0 + h - 1) / tile_size;

    let mut tiles = Vec::with_capacity(4);
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let coord = TileCoord::new(layer.mip_level, tx, ty, tile_size, layer.size.0.max(1), layer.size.1.max(1));
            if let Some(tile) = layer.store.read_tile(coord)? {
                tiles.push((coord, tile.data));
            }
        }
    }
    Ok(tiles)
}

// ---------------------------------------------------------------------------
// Rectangle intersection
// ---------------------------------------------------------------------------

/// Intersect composition tile with layer bounding box.
/// Returns `(out_x, out_y, out_w, out_h, layer_x0, layer_y0)` in pixel coords.
fn intersect_tile_layer(
    coord: TileCoord,
    _tile_size: u32,
    layer: &LayerView<'_>,
) -> Option<(u32, u32, u32, u32, u32, u32)> {
    let cx = coord.px as i64;
    let cy = coord.py as i64;
    let cw = coord.width as i64;
    let ch = coord.height as i64;

    let lx = layer.offset.0 as i64;
    let ly = layer.offset.1 as i64;
    let lw = layer.size.0 as i64;
    let lh = layer.size.1 as i64;

    let x0 = cx.max(lx);
    let y0 = cy.max(ly);
    let x1 = (cx + cw).min(lx + lw);
    let y1 = (cy + ch).min(ly + lh);

    if x0 >= x1 || y0 >= y1 {
        return None;
    }

    Some((
        (x0 - cx) as u32,       // out_x
        (y0 - cy) as u32,       // out_y
        (x1 - x0) as u32,       // out_w
        (y1 - y0) as u32,       // out_h
        (x0 - lx) as u32,       // layer_x0
        (y0 - ly) as u32,       // layer_y0
    ))
}

// ---------------------------------------------------------------------------
// Blend math
// ---------------------------------------------------------------------------

fn to_rgba_f32(px: Rgba<f16>) -> Rgba<f32> {
    Rgba::new(px.r.to_f32(), px.g.to_f32(), px.b.to_f32(), px.a.to_f32())
}

fn from_rgba_f32(px: Rgba<f32>) -> Rgba<f16> {
    Rgba::new(f16::from_f32(px.r), f16::from_f32(px.g), f16::from_f32(px.b), f16::from_f32(px.a))
}

#[inline]
fn over_blend(src: Rgba<f32>, dst: Rgba<f32>, opacity: f32) -> Rgba<f32> {
    let a = src.a * opacity;
    if a <= 1e-6 { return dst; }
    let one_minus_a = 1.0 - a;
    Rgba::new(
        src.r * opacity + dst.r * one_minus_a,
        src.g * opacity + dst.g * one_minus_a,
        src.b * opacity + dst.b * one_minus_a,
        a              + dst.a * one_minus_a,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Tile;
    use crate::pixel::Rgba;
    use crate::storage::WorkingWriter;
    use half::f16;
    use uuid::Uuid;

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("pixors").join(Uuid::new_v4().to_string())
    }

    fn make_store_solid(
        color: Rgba<f16>,
        tile_size: u32,
        image_w: u32,
        image_h: u32,
    ) -> (WorkingWriter, std::path::PathBuf) {
        let dir = temp_dir();
        let store = WorkingWriter::new(dir.clone(), tile_size, image_w, image_h).unwrap();
        let tiles_x = (image_w + tile_size - 1) / tile_size;
        let tiles_y = (image_h + tile_size - 1) / tile_size;
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let coord = TileCoord::new(0, tx, ty, tile_size, image_w, image_h);
                let n = coord.pixel_count();
                let data = vec![color; n];
                let tile = Tile::new(coord, data);
                store.write_tile_f16(&tile).unwrap();
            }
        }
        (store, dir)
    }

    fn layer_view<'a>(
        store: &'a WorkingWriter,
        size: (u32, u32),
        offset: (i32, i32),
        opacity: f32,
    ) -> LayerView<'a> {
        LayerView { id: Uuid::new_v4(), store, size, offset, opacity, blend: BlendMode::Normal, mip_level: 0 }
    }

    #[test]
    fn over_blend_two_solid_layers() {
        let (bot, _d1) = make_store_solid(
            Rgba::new(f16::ONE, f16::ZERO, f16::ZERO, f16::ONE), 16, 16, 16,
        );
        let (top, _d2) = make_store_solid(
            Rgba::new(f16::ZERO, f16::from_f32(0.5), f16::ZERO, f16::from_f32(0.5)),
            16, 16, 16,
        );
        let v_bot = layer_view(&bot, (16, 16), (0, 0), 1.0);
        let v_top = layer_view(&top, (16, 16), (0, 0), 1.0);
        let req = CompositeRequest {
            layers: &[v_bot, v_top],
            coord: TileCoord::new(0, 0, 0, 16, 16, 16),
            tile_size: 16,
        };
        let out = composite_tile(&req).unwrap();
        let p = out[0];
        assert!((p.r.to_f32() - 0.5).abs() < 0.02, "r={}", p.r.to_f32());
        assert!((p.g.to_f32() - 0.5).abs() < 0.02, "g={}", p.g.to_f32());
        assert!(p.b.to_f32() < 0.01);
        assert!((p.a.to_f32() - 1.0).abs() < 0.02, "a={}", p.a.to_f32());
    }

    #[test]
    fn opacity_zero_skips_layer() {
        let (bot, _d1) = make_store_solid(
            Rgba::new(f16::ONE, f16::ZERO, f16::ZERO, f16::ONE), 8, 8, 8,
        );
        let (top, _d2) = make_store_solid(
            Rgba::new(f16::ZERO, f16::ZERO, f16::ONE, f16::ONE), 8, 8, 8,
        );
        let v_bot = layer_view(&bot, (8, 8), (0, 0), 1.0);
        let v_top = layer_view(&top, (8, 8), (0, 0), 0.0);
        let req = CompositeRequest {
            layers: &[v_bot, v_top],
            coord: TileCoord::new(0, 0, 0, 8, 8, 8),
            tile_size: 8,
        };
        let out = composite_tile(&req).unwrap();
        let p = out[0];
        assert!(p.r.to_f32() > 0.9);
        assert!(p.g.to_f32() < 0.01);
    }

    #[test]
    fn empty_layer_list_returns_transparent_tile() {
        let req = CompositeRequest {
            layers: &[],
            coord: TileCoord::new(0, 0, 0, 4, 4, 4),
            tile_size: 4,
        };
        let out = composite_tile(&req).unwrap();
        for px in &out {
            assert!(px.r.to_f32() < 0.01);
            assert!(px.g.to_f32() < 0.01);
            assert!(px.b.to_f32() < 0.01);
            assert!(px.a.to_f32() < 0.01);
        }
    }

    #[test]
    fn invisible_layer_skipped() {
        let (bot, _d1) = make_store_solid(
            Rgba::new(f16::ONE, f16::ZERO, f16::ZERO, f16::ONE), 8, 8, 8,
        );
        let v_bot = layer_view(&bot, (8, 8), (0, 0), 1.0);
        let req = CompositeRequest {
            layers: &[v_bot],
            coord: TileCoord::new(0, 0, 0, 8, 8, 8),
            tile_size: 8,
        };
        let out = composite_tile(&req).unwrap();
        let p = out[0];
        assert!(p.r.to_f32() > 0.9);
    }

    // --- Offset and overlap tests (step 7) ---

    #[test]
    fn offset_layer_partial_overlap() {
        // 16×16 composition, layer at offset (8, 0) with size 16×16.
        // The left half of the comp tile (px 0..7) has no layer → transparent.
        // The right half (px 8..15) has red opaque.
        let (layer, _d) = make_store_solid(
            Rgba::new(f16::ONE, f16::ZERO, f16::ZERO, f16::ONE), 16, 16, 16,
        );
        let v = layer_view(&layer, (16, 16), (8, 0), 1.0);
        let req = CompositeRequest {
            layers: &[v],
            coord: TileCoord::new(0, 0, 0, 16, 16, 16),
            tile_size: 16,
        };
        let out = composite_tile(&req).unwrap();
        // Pixel at (0,0) — no layer → transparent
        assert!(out[0].a.to_f32() < 0.01, "left edge should be transparent");
        // Pixel at (8,0) — layer starts here
        assert!(out[8].r.to_f32() > 0.9, "right edge should be red");
    }

    #[test]
    fn nonaligned_layer_size() {
        // Layer is 10×10, composition tile is 16×16.
        // Pixels 10..15 in x and y should be transparent.
        let (layer, _d) = make_store_solid(
            Rgba::new(f16::ONE, f16::ONE, f16::ZERO, f16::ONE), 16, 10, 10,
        );
        let v = layer_view(&layer, (10, 10), (0, 0), 1.0);
        let req = CompositeRequest {
            layers: &[v],
            coord: TileCoord::new(0, 0, 0, 16, 16, 16),
            tile_size: 16,
        };
        let out = composite_tile(&req).unwrap();
        // Pixel at (0,0) — inside layer
        assert!(out[0].r.to_f32() > 0.9);
        // Pixel at (15,0) — outside layer width
        let idx15 = 15;
        assert!(out[idx15].a.to_f32() < 0.01, "x=15 should be outside layer");
    }
}
