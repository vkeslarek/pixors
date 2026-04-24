//! I/O modules for image formats.

use crate::color::ColorSpace;
use crate::convert::simd::convert_buffer_row_to_acescg_simd;
use crate::error::Error;
use crate::image::{Tile, TileCoord, ImageBuffer};
use crate::pixel::Rgba;
use crate::storage::TileStore;
use half::f16;

pub mod png;

/// Stream any ImageBuffer into ACEScg f16 tiles in the TileStore.
///
/// Format‑agnostic: works for interleaved, planar, gray, RGB, RGBA, etc.
/// Uses `convert_buffer_row_to_acescg_simd` which reads via BufferDesc → PlaneDesc.
pub fn stream_image_buffer_to_tiles(
    source: &ImageBuffer,
    tile_size: u32,
    store: &TileStore,
) -> Result<(), Error> {
    let w = source.desc.width;
    let h = source.desc.height;
    let conv = source.desc.color_space.converter_to(ColorSpace::ACES_CG)?;
    let tiles_x = (w + tile_size - 1) / tile_size;

    let mut band_buf: Vec<Rgba<f16>> = vec![Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO); (w * tile_size) as usize];
    let mut rows_in_band = 0u32;
    let mut band_start_y = 0u32;

    for row_y in 0..h {
        let start = (rows_in_band * w) as usize;
        let end   = start + w as usize;
        convert_buffer_row_to_acescg_simd(source, row_y, &mut band_buf[start..end], &conv);
        rows_in_band += 1;

        let is_last = row_y == h - 1;
        if rows_in_band == tile_size || is_last {
            let actual_rows = rows_in_band;
            for tx in 0..tiles_x {
                let tile_px = tx * tile_size;
                let actual_w = (w - tile_px).min(tile_size);
                let coord = TileCoord::new(0, tx, band_start_y / tile_size, tile_size, w, h);
                let mut tile_data = Vec::with_capacity((actual_w * actual_rows) as usize);
                for r in 0..actual_rows {
                    let src_start = (r * w + tile_px) as usize;
                    tile_data.extend_from_slice(&band_buf[src_start..src_start + actual_w as usize]);
                }
                store.write_tile_blocking(&Tile::new(coord, tile_data))?;
            }
            rows_in_band = 0;
            band_start_y = row_y + 1;
        }
    }

    Ok(())
}
