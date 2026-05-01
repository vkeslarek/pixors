use crate::image::{Tile, TileCoord};
use crate::pixel::Rgba;
use crate::pipeline::emitter::Emitter;
use crate::pipeline::source::Source;
use crate::storage::WorkingWriter;
use half::f16;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Reads all tiles from a WorkingWriter across all MIP levels.
pub struct WorkSource {
    store: Arc<WorkingWriter>,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
}

impl WorkSource {
    pub fn new(store: Arc<WorkingWriter>, tile_size: u32, image_width: u32, image_height: u32) -> Self {
        Self { store, tile_size, image_width, image_height }
    }
}

impl Source for WorkSource {
    type Item = Tile<Rgba<f16>>;

    fn run(self, emit: &mut Emitter<Self::Item>, cancel: Arc<AtomicBool>) {
        let max_mip = (self.image_width.max(self.image_height) as f32).log2().ceil() as u32;
        for mip in 0..=max_mip {
            if cancel.load(Ordering::Relaxed) { return; }
            let iw = (self.image_width >> mip).max(1);
            let ih = (self.image_height >> mip).max(1);
            for ty in 0..ih.div_ceil(self.tile_size) {
                for tx in 0..iw.div_ceil(self.tile_size) {
                    if cancel.load(Ordering::Relaxed) { return; }
                    let coord = TileCoord::new(mip, tx, ty, self.tile_size, iw, ih);
                    if let Ok(Some(tile)) = self.store.read_tile(coord) {
                        emit.emit(tile);
                    }
                }
            }
        }
    }
}
