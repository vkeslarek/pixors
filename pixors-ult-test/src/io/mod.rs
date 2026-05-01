//! I/O modules for image formats.

use crate::error::Error;
use crate::image::{Image, Layer, LayerMetadata, ImageInfo, TileCoord};
use crate::storage::writer::TileWriter;
use std::path::Path;

pub mod png;
pub mod tiff;
pub mod accumulator;

// ---------------------------------------------------------------------------
// ImageReader trait — layer-aware
// ---------------------------------------------------------------------------

pub trait ImageReader: Send + Sync {
    fn can_handle(&self, path: &Path) -> bool;

    /// Number of layers and document-level metadata (no pixel decode).
    fn read_document_info(&self, path: &Path) -> Result<ImageInfo, Error>;

    /// Per-layer metadata — no pixel decode.
    fn read_layer_metadata(&self, path: &Path, layer: usize) -> Result<LayerMetadata, Error>;

    /// Decode one layer in full.
    fn load_layer(&self, path: &Path, layer: usize) -> Result<Layer, Error>;

    /// Stream-decode one layer as raw bytes to a `TileWriter<u8>`.
    /// The writer handles color conversion internally.
    ///
    /// Default implementation falls back to `load_layer` + band-by-band emit.
    /// Formats that support streaming decode (PNG) override this.
    fn stream_tiles(
        &self,
        path: &Path,
        tile_size: u32,
        writer: &dyn TileWriter<u8>,
        layer: usize,
        on_progress: Option<&(dyn Fn(u8) + Send)>,
    ) -> Result<(), Error> {
        // Default: load full layer, emit raw bytes band-by-band
        let layer_data = self.load_layer(path, layer)?;
        let buf = &layer_data.buffer;
        let w = buf.desc.width;
        let h = buf.desc.height;
        let tiles_x = w.div_ceil(tile_size);
        let tiles_y = h.div_ceil(tile_size);

        for band_ty in 0..tiles_y {
            let band_start_y = band_ty * tile_size;
            let band_height = (h - band_start_y).min(tile_size);

            for tx in 0..tiles_x {
                let tile_px = tx * tile_size;
                let actual_w = (w - tile_px).min(tile_size);
                let coord = TileCoord::new(0, tx, band_ty, tile_size, w, h);

                let channels = buf.desc.planes.len();
                let bytes_per_pixel = channels * buf.desc.planes[0].encoding.byte_size();
                let mut tile_data = vec![0u8; (actual_w * band_height) as usize * bytes_per_pixel];

                let row_stride = w as usize * bytes_per_pixel;
                let tile_stride = actual_w as usize * bytes_per_pixel;
                for r in 0..band_height as usize {
                    let src_start = ((band_start_y as usize + r) * row_stride) + tile_px as usize * bytes_per_pixel;
                    let dst_start = r * tile_stride;
                    tile_data[dst_start..dst_start + tile_stride]
                        .copy_from_slice(&buf.data[src_start..src_start + tile_stride]);
                }

                writer.write_tile(coord, &tile_data)?;
            }
            if let Some(cb) = on_progress {
                cb(((band_ty + 1) * 100 / tiles_y) as u8);
            }
        }
        writer.finish()?;
        Ok(())
    }

    /// Convenience: decode the whole document.
    fn load_document(&self, path: &Path) -> Result<Image, Error> {
        let info = self.read_document_info(path)?;
        let layers = (0..info.layer_count)
            .map(|i| self.load_layer(path, i))
            .collect::<Result<_, _>>()?;
        Ok(Image { layers, metadata: info.metadata })
    }
}

/// All registered image formats, in priority order.
pub fn all_readers() -> &'static [&'static dyn ImageReader] {
    &[&png::PngFormat, &tiff::TiffFormat]
}
