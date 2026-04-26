//! Streaming decode helpers: accumulate scanlines into tile-sized bands.
//!
//! `RowAccumulator` enables streaming IO without allocating a full-image buffer.

use crate::image::buffer::BufferDesc;
use crate::image::TileCoord;

pub struct TileFragment<'a> {
    pub coord: TileCoord,
    pub data: Box<[u8]>,
    pub desc: &'a BufferDesc,
}

pub struct RowAccumulator {
    buf: Box<[u8]>,
    desc: BufferDesc,
    row_stride: usize,
    bpp: usize,
    tile_dim: u32,
    rows_filled: u32,
    band_ty: u32,
    image_width: u32,
    image_height: u32,
}

impl RowAccumulator {
    pub fn new(width: u32, height: u32, tile_dim: u32, desc: BufferDesc, max_band_bytes: usize) -> Self {
        let channels = desc.planes.len();
        let bpp = channels * desc.planes[0].encoding.byte_size();
        let mut effective_tile_h = tile_dim;

        let band_bytes = width as usize * tile_dim as usize * bpp;
        if band_bytes > max_band_bytes {
            let max_rows = max_band_bytes / (width as usize * bpp).max(1);
            effective_tile_h = max_rows.max(1) as u32;
        }

        let buf_size = width as usize * effective_tile_h as usize * bpp;
        let row_stride = width as usize * bpp;
        let mut buf = Vec::with_capacity(buf_size);
        buf.resize(buf_size, 0u8);

        Self {
            buf: buf.into_boxed_slice(),
            desc,
            row_stride,
            bpp,
            tile_dim: effective_tile_h,
            rows_filled: 0,
            band_ty: 0,
            image_width: width,
            image_height: height,
        }
    }

    pub fn push_row(&mut self, row_data: &[u8]) -> u32 {
        let row = self.rows_filled;
        let start = row as usize * self.row_stride;
        let end = start + self.row_stride.min(row_data.len());
        self.buf[start..end].copy_from_slice(&row_data[..end - start]);
        self.rows_filled += 1;
        row
    }

    pub fn is_full(&self) -> bool { self.rows_filled >= self.tile_dim }

    /// Extract tile fragments from the current band. `band_ty` is the band index.
    pub fn extract_tiles(&self) -> Vec<TileFragment<'_>> {
        let band_height = self.rows_filled;
        let tiles_x = (self.image_width + self.tile_dim - 1) / self.tile_dim;
        let mut fragments = Vec::with_capacity(tiles_x as usize);

        for tx in 0..tiles_x {
            let tile_px = tx * self.tile_dim;
            let actual_w = (self.image_width - tile_px).min(self.tile_dim);
            let buf_size = actual_w as usize * band_height as usize * self.bpp;
            let tile_stride = actual_w as usize * self.bpp;

            let mut tile_buf = Vec::with_capacity(buf_size);
            tile_buf.resize(buf_size, 0u8);

            for r in 0..band_height as usize {
                let src_start = r * self.row_stride + tile_px as usize * self.bpp;
                let dst_start = r * tile_stride;
                tile_buf[dst_start..dst_start + tile_stride]
                    .copy_from_slice(&self.buf[src_start..src_start + tile_stride]);
            }

            let coord = TileCoord::new(0, tx, self.band_ty, self.tile_dim, self.image_width, self.image_height);
            fragments.push(TileFragment {
                coord,
                data: tile_buf.into_boxed_slice(),
                desc: &self.desc,
            });
        }
        fragments
    }

    pub fn reset(&mut self) {
        self.rows_filled = 0;
        self.band_ty += 1;
    }

    pub fn rows_filled(&self) -> u32 { self.rows_filled }
    pub fn band_ty(&self) -> u32 { self.band_ty }
    pub fn effective_tile_height(&self) -> u32 { self.tile_dim }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorSpace;
    use crate::image::buffer::BufferDesc;
    use crate::image::AlphaMode;

    #[test]
    fn accumulator_basic() {
        let desc = BufferDesc::rgb8_interleaved(4, 4, ColorSpace::SRGB, AlphaMode::Opaque);
        let mut acc = RowAccumulator::new(4, 4, 2, desc, 64 * 1024 * 1024);
        let row: Vec<u8> = vec![0u8; 4 * 3];
        acc.push_row(&row);
        assert!(!acc.is_full());
        acc.push_row(&row);
        assert!(acc.is_full());
        let fragments = acc.extract_tiles();
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].data.len(), 2 * 2 * 3);
    }

    #[test]
    fn accumulator_clamps_tile_height() {
        let desc = BufferDesc::rgba8_interleaved(100, 100, ColorSpace::SRGB, AlphaMode::Straight);
        let acc = RowAccumulator::new(100, 100, 256, desc, 1024);
        assert!(acc.effective_tile_height() < 256);
    }
}
