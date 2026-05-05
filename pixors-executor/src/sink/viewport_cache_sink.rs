use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};

pub type CacheCommitFn = Box<dyn Fn(u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;

static CACHE_SINK: OnceLock<Arc<CacheCommitFn>> = OnceLock::new();

pub fn install_viewport_cache_sink(f: CacheCommitFn) {
    let _ = CACHE_SINK.set(Arc::new(f));
}

pub fn viewport_cache_sink() -> Option<Arc<CacheCommitFn>> {
    CACHE_SINK.get().cloned()
}

static VCS_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static VCS_OUTPUTS: &[PortDeclaration] = &[];
static VCS_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(VCS_INPUTS),
    outputs: PortGroup::Fixed(VCS_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportCacheSink;

impl Stage for ViewportCacheSink {
    fn kind(&self) -> &'static str {
        "viewport_cache_sink"
    }

    fn ports(&self) -> &'static PortSpecification {
        &VCS_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        let cb = CACHE_SINK.get().cloned()?;
        Some(Box::new(ViewportCacheSinkProcessor { cb }))
    }
}

pub struct ViewportCacheSinkProcessor {
    cb: Arc<CacheCommitFn>,
}

impl Processor for ViewportCacheSinkProcessor {
    fn process(&mut self, _ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let tile = ProcessorContext::take_tile(item)?;
        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("ViewportCacheSink requires CPU tiles")),
        };
        (self.cb)(
            tile.coord.mip_level,
            tile.coord.tx,
            tile.coord.ty,
            tile.coord.px,
            tile.coord.py,
            tile.coord.width,
            tile.coord.height,
            data,
        );
        Ok(())
    }
}
