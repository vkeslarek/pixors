use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    Consumer, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage,
};

pub type TileCommitFn = Box<dyn Fn(u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;

static TILE_SINK: OnceLock<Arc<TileCommitFn>> = OnceLock::new();

pub fn install_tile_sink(f: TileCommitFn) {
    let _ = TILE_SINK.set(Arc::new(f));
}

pub fn tile_sink() -> Option<Arc<TileCommitFn>> {
    TILE_SINK.get().cloned()
}

static TS_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static TS_OUTPUTS: &[PortDeclaration] = &[];

static TS_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(TS_INPUTS),
    outputs: PortGroup::Fixed(TS_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileSink;

impl Stage for TileSink {
    fn kind(&self) -> &'static str {
        "tile_sink"
    }

    fn ports(&self) -> &'static PortSpecification {
        &TS_PORTS
    }

    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        let cb = TILE_SINK.get().cloned()?;
        Some(Box::new(TileSinkConsumer { cb }))
    }
}

pub struct TileSinkConsumer {
    cb: Arc<TileCommitFn>,
}

impl Consumer for TileSinkConsumer {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = crate::stage::ProcessorContext::take_tile(item)?;
        let src: &[u8] = match &tile.data {
            crate::data::buffer::Buffer::Cpu(v) => v.as_slice(),
            crate::data::buffer::Buffer::Gpu(_) => {
                return Err(Error::internal("GPU tile not supported in tile_sink"));
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
}
