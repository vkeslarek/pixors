use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage};

pub type CacheCommitFn = Box<dyn Fn(u64, u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;
//       gen, mip,  tx,  ty,  px,  py,   w,   h, bytes

static CACHE_ROUTER: RwLock<Option<HashMap<u64, Arc<CacheCommitFn>>>> = RwLock::new(None);

pub fn install_router() {
    let mut w = CACHE_ROUTER.write().unwrap();
    if w.is_none() {
        *w = Some(HashMap::new());
    }
}

pub fn register_tab_cache(key: u64, f: CacheCommitFn) {
    let mut w = CACHE_ROUTER.write().unwrap();
    w.get_or_insert_with(HashMap::new).insert(key, Arc::new(f));
}

pub fn unregister_tab_cache(key: u64) {
    if let Some(ref mut map) = *CACHE_ROUTER.write().unwrap() {
        map.remove(&key);
    }
}

fn lookup_cb(key: u64) -> Option<Arc<CacheCommitFn>> {
    CACHE_ROUTER
        .read()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(&key).cloned())
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
pub struct ViewportCacheSink {
    pub routing_key: u64,
    pub generation: u64,
}

impl ViewportCacheSink {
    pub fn new(routing_key: u64, generation: u64) -> Self {
        Self {
            routing_key,
            generation,
        }
    }
}

impl Stage for ViewportCacheSink {
    fn kind(&self) -> &'static str {
        "viewport_cache_sink"
    }

    fn ports(&self) -> &'static PortSpecification {
        &VCS_PORTS
    }

    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        let cb = lookup_cb(self.routing_key)?;
        Some(Box::new(ViewportCacheSinkConsumer {
            cb,
            generation: self.generation,
        }))
    }
}

pub struct ViewportCacheSinkConsumer {
    cb: Arc<CacheCommitFn>,
    generation: u64,
}

impl Consumer for ViewportCacheSinkConsumer {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("ViewportCacheSink requires CPU tiles")),
        };
        (self.cb)(
            self.generation,
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
