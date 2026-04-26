//! Tile-level conversion: converts an ImageBuffer into ACEScg f16 tiles directly.
//!
//! Replaces `io/mod.rs::stream_image_buffer_to_tiles`.
//! Writes tiles directly (no band buffer) via `convert_row_strided`.

use crate::color::ColorConversion;
use crate::error::Error;
use crate::image::{ImageBuffer, Tile, TileCoord};
use crate::pixel::{AlphaPolicy, Rgba};
use crate::storage::TileStore;
use half::f16;
use rayon::prelude::*;

/// Convert an entire `ImageBuffer` into ACEScg f16 tiles and write them to `store`.
///
/// Band-by-band processing: no full-image buffer needed.
/// `on_progress` is called with percent (0–100) after each band completes.
pub fn convert_to_tiles(
    conv: &ColorConversion,
    source: &ImageBuffer,
    tile_size: u32,
    store: &TileStore,
    on_progress: Option<&(dyn Fn(u8) + Send)>,
) -> Result<(), Error> {
    let w = source.desc.width;
    let h = source.desc.height;
    let tiles_x = (w + tile_size - 1) / tile_size;
    let tiles_y = (h + tile_size - 1) / tile_size;

    for band_ty in 0..tiles_y {
        let band_start_y = band_ty * tile_size;
        let band_height = (h - band_start_y).min(tile_size);

        // Process tile columns in parallel within this band.
        // Each tile column is independent — no shared mutable state.
        (0..tiles_x).into_par_iter().try_for_each(|tx| {
            let tile_px = tx * tile_size;
            let actual_w = (w - tile_px).min(tile_size);

            let coord = TileCoord::new(0, tx, band_ty, tile_size, w, h);
            let mut tile_data: Vec<Rgba<f16>> =
                vec![Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO); (actual_w * band_height) as usize];

            for r in 0..band_height {
                let dst_start = (r * actual_w) as usize;
                let dst_end = dst_start + actual_w as usize;
                conv.convert_row_strided::<Rgba<f16>>(
                    source,
                    band_start_y + r,
                    tile_px,
                    tile_px + actual_w,
                    &mut tile_data[dst_start..dst_end],
                    AlphaPolicy::PremultiplyOnPack,
                );
            }

            store.write_tile_blocking(&Tile::new(coord, tile_data))
        })?;

        // Report progress after each band
        if let Some(cb) = on_progress {
            let percent = ((band_ty + 1) * 100 / tiles_y) as u8;
            cb(percent);
        }
    }

    Ok(())
}
