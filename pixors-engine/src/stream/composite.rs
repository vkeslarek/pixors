use crate::stream::{Frame, Pipe};
use std::sync::mpsc;

/// Composites tiles from N layers into a single tile stream.
/// Currently single-layer only (acts as identity). Multi-layer future work.
pub struct CompositePipe {
    // Future: layer_count, barrio state for multi-layer
}

impl CompositePipe {
    pub fn new() -> Self {
        Self {}
    }
}

impl Pipe for CompositePipe {
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame> {
        let (tx, out) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                // Single-layer: pass through unchanged
                // Multi-layer TODO: barrio + composite_tile_ram
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });
        out
    }
}
