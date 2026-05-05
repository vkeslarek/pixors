use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortGroup, PortSpec, Stage, StageHints};

use crate::graph::emitter::Emitter;

use crate::graph::item::Item;

use crate::error::Error;

use crate::debug_stopwatch;


/// Callback: invoked when a tile arrives with its pixel coordinates and RGBA8 bytes.

pub type TileCommitFn = Box<dyn Fn(u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;


static TILE_SINK: OnceLock<Arc<TileCommitFn>> = OnceLock::new();


pub fn install_tile_sink(f: TileCommitFn) {

    let _ = TILE_SINK.set(Arc::new(f));

}


pub fn tile_sink() -> Option<Arc<TileCommitFn>> {

    TILE_SINK.get().cloned()

}


static TS_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];

static TS_OUTPUTS: &[PortDecl] = &[];

static TS_PORTS: PortSpec = PortSpec { inputs: PortGroup::Fixed(TS_INPUTS), outputs: PortGroup::Fixed(TS_OUTPUTS) };

// ── Stage ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileSink;

impl Stage for TileSink {
    fn kind(&self) -> &'static str {
        "tile_sink"
    }

    fn ports(&self) -> &'static PortSpec {
        &TS_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        let cb = TILE_SINK.get().cloned()?;
        Some(Box::new(TileSinkRunner { cb }))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

pub struct TileSinkRunner {
    cb: Arc<TileCommitFn>,
}

impl CpuKernel for TileSinkRunner {
    fn process(&mut self, _port: u16, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
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
                    tile.coord.mip_level,
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
