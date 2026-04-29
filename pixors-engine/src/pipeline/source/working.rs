// ── WorkSource — skeleton for Phase 9 operations ──

use std::sync::{mpsc, Arc};
use half::f16;
use crate::error::Error;
use crate::image::TileCoord;
use crate::Rgba;
use crate::stream::{Frame, FrameKind, FrameMeta, TileSource};

/// A source that emits pre-computed tiles (e.g., from operations).
pub struct WorkSource {
    pub tiles: Vec<(TileCoord, Arc<Vec<Rgba<f16>>>)>,
    pub meta: FrameMeta,
}

impl WorkSource {
    pub fn new(meta: FrameMeta) -> Self {
        Self { tiles: Vec::new(), meta }
    }

    pub fn add_tile(&mut self, coord: TileCoord, data: Arc<Vec<Rgba<f16>>>) {
        self.tiles.push((coord, data));
    }
}

impl TileSource for WorkSource {
    fn open(self) -> Result<mpsc::Receiver<Frame>, Error> {
        let (tx, rx) = mpsc::sync_channel::<Frame>(64);
        std::thread::spawn(move || {
            for (coord, data) in self.tiles.into_iter() {
                let bytes = bytemuck::cast_slice::<Rgba<f16>, u8>(&data).to_vec();
                let frame = Frame::new(self.meta, FrameKind::Tile { coord }, bytes);
                if tx.send(frame).is_err() { break; }
            }
            let _ = tx.send(Frame::new(self.meta, FrameKind::StreamDone, vec![]));
        });
        Ok(rx)
    }
}
