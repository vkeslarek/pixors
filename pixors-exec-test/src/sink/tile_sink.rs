use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::data::Device;
use crate::stage::{Stage, StageRole};
use crate::graph::item::Item;
use crate::graph::runner::SinkRunner;
use crate::error::Error;
use crate::debug_stopwatch;

/// Callback: invoked when a tile arrives with its pixel coordinates and RGBA8 bytes.
pub type TileCommitFn = Box<dyn Fn(u32, u32, u32, u32, &[u8]) + Send + Sync>;

static TILE_SINK: OnceLock<Arc<TileCommitFn>> = OnceLock::new();

pub fn install_tile_sink(f: TileCommitFn) {
    let _ = TILE_SINK.set(Arc::new(f));
}

pub fn tile_sink() -> Option<Arc<TileCommitFn>> {
    TILE_SINK.get().cloned()
}

// ── Stage ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileSink;

impl Stage for TileSink {
    fn kind(&self) -> &'static str {
        "tile_sink"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        false
    }
    fn role(&self) -> StageRole {
        StageRole::Sink
    }
    fn sink_runner(&self) -> Result<Box<dyn SinkRunner>, Error> {
        let cb = TILE_SINK
            .get()
            .cloned()
            .ok_or_else(|| Error::internal("tile sink not installed"))?;
        Ok(Box::new(TileSinkRunner { cb }))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

pub struct TileSinkRunner {
    cb: Arc<TileCommitFn>,
}

impl SinkRunner for TileSinkRunner {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("tile_sink:consume");
        match item {
            Item::Tile(tile) => {
                let src: &[u8] = match &tile.data {
                    crate::data::Buffer::Cpu(v) => v.as_slice(),
                    crate::data::Buffer::Gpu(_) => {
                        return Err(Error::internal("GPU tile not supported in tile_sink"))
                    }
                };
                (self.cb)(
                    tile.coord.px,
                    tile.coord.py,
                    tile.coord.width,
                    tile.coord.height,
                    src,
                );
                Ok(())
            }
            _ => Err(Error::internal("expected Tile")),
        }
    }
}
