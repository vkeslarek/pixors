//! I/O modules for image formats.

use crate::color::ColorSpace;
use crate::convert::simd::convert_buffer_row_to_acescg_simd;
use crate::error::Error;
use crate::image::{Tile, TileCoord, ImageBuffer, AlphaMode};
use crate::pixel::Rgba;
use crate::storage::TileStore;
use half::f16;
use rayon::prelude::*;
use std::path::Path;

pub mod png;
pub mod tiff;

// ---------------------------------------------------------------------------
// ImageReader trait — any image format that can be loaded
// ---------------------------------------------------------------------------

/// Format-agnostic image reader. Implemented per format (PNG, TIFF, etc.).
/// Stateless — `&self` is ignored, all logic is per-call.
pub trait ImageReader: Send + Sync {
    /// Returns true if this reader believes it can decode the given file.
    fn can_handle(&self, path: &Path) -> bool;

    /// Read image dimensions and color metadata without full decode.
    fn read_metadata(&self, path: &Path) -> Result<(u32, u32, ColorSpace, AlphaMode), Error>;

    /// Load the full image into an ImageBuffer.
    fn load(&self, path: &Path) -> Result<ImageBuffer, Error>;

    /// Load the image and stream ACEScg f16 tiles directly into a TileStore.
    fn stream_to_tiles_sync(
        &self,
        path: &Path,
        width: u32,
        height: u32,
        tile_size: u32,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        store: &TileStore,
        on_progress: Option<&(dyn Fn(u8) + Send)>,
    ) -> Result<(), Error>;
}

/// All registered image formats, in priority order.
pub fn all_readers() -> &'static [&'static dyn ImageReader] {
    &[&png::PngFormat, &tiff::TiffFormat]
}

// ---------------------------------------------------------------------------
// Shared tile streaming (format-agnostic)
// ---------------------------------------------------------------------------

/// Stream any ImageBuffer into ACEScg f16 tiles in the TileStore.
///
/// Format‑agnostic: works for interleaved, planar, gray, RGB, RGBA, etc.
/// Band-by-band processing: no full-image buffer, writes L0 tiles as bands complete.
/// Much lower memory usage than full-buffer approach.
///
/// `on_progress`: optional callback fired after each band, receives percent (0–100).
pub fn stream_image_buffer_to_tiles(
    source: &ImageBuffer,
    tile_size: u32,
    store: &TileStore,
    on_progress: Option<&(dyn Fn(u8) + Send)>,
) -> Result<(), Error> {
    let w = source.desc.width;
    let h = source.desc.height;
    let conv = source.desc.color_space.converter_to(ColorSpace::ACES_CG)?;
    let tiles_x = (w + tile_size - 1) / tile_size;
    let tiles_y = (h + tile_size - 1) / tile_size;

    let t_start = std::time::Instant::now();

    // Process band-by-band: tile_size rows per band
    for band_ty in 0..tiles_y {
        let band_start_y = band_ty * tile_size;
        let band_height = (h - band_start_y).min(tile_size);

        // Convert band in parallel (rayon per row)
        let mut band_buf: Vec<Rgba<f16>> = vec![
            Rgba::new(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO);
            (w * band_height) as usize
        ];

        band_buf
            .par_chunks_exact_mut(w as usize)
            .enumerate()
            .for_each(|(local_row, row_slice)| {
                convert_buffer_row_to_acescg_simd(
                    source,
                    band_start_y + local_row as u32,
                    row_slice,
                    &conv,
                );
            });

        // Write L0 tiles for this band immediately
        for tx in 0..tiles_x {
            let tile_px = tx * tile_size;
            let actual_w = (w - tile_px).min(tile_size);
            let coord = TileCoord::new(0, tx, band_ty, tile_size, w, h);
            let mut tile_data = Vec::with_capacity((actual_w * band_height) as usize);

            for r in 0..band_height {
                let src_start = ((r * w) + tile_px) as usize;
                tile_data.extend_from_slice(&band_buf[src_start..src_start + actual_w as usize]);
            }

            store.write_tile_blocking(&Tile::new(coord, tile_data))?;
        }

        // Report progress after each band
        if let Some(cb) = on_progress {
            let percent = ((band_ty + 1) * 100 / tiles_y) as u8;
            cb(percent);
        }
    }

    let elapsed = t_start.elapsed();
    tracing::debug!(
        "stream_tiles: done total={:.3}s tiles={}x{}={}",
        elapsed.as_secs_f64(), tiles_x, tiles_y, tiles_x * tiles_y
    );

    Ok(())
}
