use std::collections::BTreeMap;

use crate::container::meta::PixelMeta;
use crate::container::{ScanLine, Tile};
use crate::egraph::emitter::Emitter;
use crate::egraph::item::Item;
use crate::egraph::runner::OperationRunner;
use crate::error::Error;
use crate::storage::Buffer;

/// Inverse of `ScanLineAccumulator`: takes Tiles back to ScanLines.
///
/// Tiles can arrive in any order — upstream stages (e.g. `NeighborhoodAgg`)
/// drain a `HashMap`, so band order is not guaranteed. We bucket tiles by
/// band (`ty`) and emit each band's scanlines on `finish` once the full
/// image is in. Per-band flushing on a `ty` change would corrupt rows when
/// columns from the same band are interleaved with another band.
pub struct TileToScanlineRunner {
    bands: BTreeMap<u32, Vec<Tile>>,
    image_width: u32,
    meta: Option<PixelMeta>,
}

impl TileToScanlineRunner {
    pub fn new() -> Self {
        Self {
            bands: BTreeMap::new(),
            image_width: 0,
            meta: None,
        }
    }
}

impl OperationRunner for TileToScanlineRunner {
    fn process(&mut self, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("expected Tile")),
        };

        if self.meta.is_none() {
            self.meta = Some(tile.meta);
        }
        self.image_width = self.image_width.max(tile.coord.px + tile.coord.width);

        self.bands.entry(tile.coord.ty).or_default().push(tile);
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let Some(meta) = self.meta else { return Ok(()) };
        let bpp = meta.format.bytes_per_pixel();
        let row_bytes = self.image_width as usize * bpp;

        // Iterate bands in ascending `ty`.
        let bands = std::mem::take(&mut self.bands);
        for (_ty, mut tiles) in bands {
            // Sort columns left-to-right within the band.
            tiles.sort_by_key(|t| t.coord.px);

            let band_py = tiles[0].coord.py;
            let band_h = tiles.iter().map(|t| t.coord.height).max().unwrap_or(0);

            for row in 0..band_h {
                let mut full_row = vec![0u8; row_bytes];
                for tile in &tiles {
                    if row >= tile.coord.height {
                        continue;
                    }
                    let data = match &tile.data {
                        Buffer::Cpu(v) => v,
                        Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
                    };
                    let tw = tile.coord.width as usize;
                    let src_off = row as usize * tw * bpp;
                    let dst_off = tile.coord.px as usize * bpp;
                    let len = (tw * bpp)
                        .min(data.len() - src_off)
                        .min(full_row.len() - dst_off);
                    full_row[dst_off..dst_off + len]
                        .copy_from_slice(&data[src_off..src_off + len]);
                }
                emit.emit(Item::ScanLine(ScanLine::new(
                    band_py + row,
                    self.image_width,
                    meta,
                    Buffer::Cpu(full_row),
                )));
            }
        }
        Ok(())
    }
}
