use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;

use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};

pub type CacheCommitFn = Box<dyn Fn(u64, u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;

static CACHE_ROUTER: LazyLock<RwLock<Option<HashMap<u64, Arc<CacheCommitFn>>>>> =
    LazyLock::new(|| RwLock::new(None));

pub fn install_router() {
    let mut w = CACHE_ROUTER.write();
    if w.is_none() { *w = Some(HashMap::new()); }
}

pub fn register_tile_cache(key: u64, f: CacheCommitFn) {
    CACHE_ROUTER.write().get_or_insert_with(HashMap::new).insert(key, Arc::new(f));
}

pub fn unregister_tile_cache(key: u64) {
    if let Some(ref mut map) = *CACHE_ROUTER.write() { map.remove(&key); }
}

fn lookup_cb(key: u64) -> Option<Arc<CacheCommitFn>> {
    CACHE_ROUTER.read().as_ref().and_then(|m| m.get(&key).cloned())
}

static VCS_INPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];
static VCS_IN_PORTS: InPortSpecification = InPortSpecification { ports: PortGroup::Fixed(VCS_INPUTS) };

#[derive(Debug, Clone)]
pub struct TileCacheSink {
    pub routing_key: u64,
    pub generation: u64,
}

impl TileCacheSink {
    pub fn new(routing_key: u64, generation: u64) -> Self { Self { routing_key, generation } }
}

impl Consumer for TileCacheSink {
    fn kind(&self) -> &'static str { "tile_cache_sink" }
    fn in_ports(&self) -> &'static InPortSpecification { &VCS_IN_PORTS }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let cb = lookup_cb(self.routing_key).ok_or_else(|| Error::internal("tile cache not registered"))?;
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("TileCacheSink requires CPU tiles")),
        };
        (cb)(self.generation, tile.coord.mip_level, tile.coord.tx, tile.coord.ty, tile.coord.px, tile.coord.py, tile.coord.width, tile.coord.height, data);
        Ok(())
    }
}
