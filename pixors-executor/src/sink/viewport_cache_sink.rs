use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    Consumer, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage,
};

pub type CacheCommitFn = Box<dyn Fn(u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;

static CACHE_SINK: RwLock<Option<Arc<CacheCommitFn>>> = RwLock::new(None);

pub fn install_viewport_cache_sink(f: CacheCommitFn) {
    *CACHE_SINK.write().unwrap() = Some(Arc::new(f));
}

pub fn uninstall_viewport_cache_sink() {
    *CACHE_SINK.write().unwrap() = None;
}

pub fn viewport_cache_sink() -> Option<Arc<CacheCommitFn>> {
    CACHE_SINK.read().unwrap().clone()
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

    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        let cb = CACHE_SINK.read().unwrap().clone()?;
        Some(Box::new(ViewportCacheSinkConsumer { cb }))
    }
}

pub struct ViewportCacheSinkConsumer {
    cb: Arc<CacheCommitFn>,
}

impl Consumer for ViewportCacheSinkConsumer {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = crate::stage::ProcessorContext::take_tile(item)?;
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
