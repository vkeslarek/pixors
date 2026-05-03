use serde::{Deserialize, Serialize};

use crate::container::meta::PixelMeta;
use crate::container::{Tile, TileCoord};
use crate::pipeline::exec_graph::emitter::Emitter;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::OperationRunner;
use super::{Device, Stage};
use crate::error::Error;
use crate::gpu::Buffer;
use crate::debug_stopwatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanLineAccumulator {
    pub tile_size: u32,
}

impl Stage for ScanLineAccumulator {
    fn kind(&self) -> &'static str { "scanline_accumulator" }
    fn device(&self) -> Device { Device::Cpu }
    fn allocates_output(&self) -> bool { true }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(ScanLineAccumulatorRunner::new(self.tile_size)))
    }
}

pub struct ScanLineAccumulatorRunner {
    tile_size: u32,
    rows: Vec<Vec<u8>>,
    meta: Option<PixelMeta>,
    band_y: u32,
    image_width: u32,
    image_height: u32,
    initialized: bool,
}

impl ScanLineAccumulatorRunner {
    pub fn new(tile_size: u32) -> Self {
        Self {
            tile_size,
            rows: vec![],
            meta: None,
            band_y: 0,
            image_width: 0,
            image_height: 0,
            initialized: false,
        }
    }
}

impl OperationRunner for ScanLineAccumulatorRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("scanline_accumulator");
        let scanline = match &item {
            Item::ScanLine(s) => s,
            _ => return Err(Error::internal("expected ScanLine")),
        };
        if !self.initialized {
            self.meta = Some(scanline.meta);
            self.image_width = scanline.width;
            self.initialized = true;
        }
        self.image_height = self.image_height.max(scanline.y + 1);
        self.rows.push(match &scanline.data {
            Buffer::Cpu(v) => (**v).clone(),
            Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
        });
        if self.rows.len() >= self.tile_size as usize {
            self.emit_tiles(emit);
        }
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        if !self.rows.is_empty() {
            self.emit_tiles(emit);
        }
        Ok(())
    }
}

impl ScanLineAccumulatorRunner {
    fn emit_tiles(&mut self, emit: &mut Emitter<Item>) {
        let meta = self.meta.unwrap();
        let rows = std::mem::take(&mut self.rows);
        let band_h = rows.len() as u32;
        let bpp = meta.format.bytes_per_pixel();
        let tiles_x = self.image_width.div_ceil(self.tile_size);

        for tx in 0..tiles_x {
            let px = tx * self.tile_size;
            let tw = ((self.image_width - px).min(self.tile_size)) as usize;
            let mut buf = Vec::with_capacity(tw * band_h as usize * bpp);
            for row in &rows {
                let s = (px as usize * bpp).min(row.len());
                let e = (s + tw * bpp).min(row.len());
                buf.extend_from_slice(&row[s..e]);
            }
            let coord = TileCoord::new(
                tx,
                self.band_y,
                self.tile_size,
                self.image_width,
                self.image_height,
            );
            emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(buf))));
        }
        self.band_y += 1;
    }
}
