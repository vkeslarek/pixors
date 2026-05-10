use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;

use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{DataKind, OutPortSpecification, PortDeclaration, PortGroup, ProcessorContext, Producer};
use pixors_ops::source::cache_reader::TileRange;

pub type TileReadFn = Box<dyn Fn(u64, u64, u32, Option<TileRange>) -> Vec<Item> + Send + Sync>;

static TILE_READERS: LazyLock<RwLock<HashMap<u64, Arc<TileReadFn>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn install_tile_cache_reader(tab_id: u64, f: TileReadFn) {
    TILE_READERS.write().insert(tab_id, Arc::new(f));
}

pub fn uninstall_tile_cache_reader(tab_id: u64) {
    TILE_READERS.write().remove(&tab_id);
}

static VCS_OUTPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];
static VCS_OUT_PORTS: OutPortSpecification = OutPortSpecification { ports: PortGroup::Fixed(VCS_OUTPUTS) };

/// Reads tiles from the in-memory tile cache registered by the desktop layer.
#[derive(Debug, Clone)]
pub struct TileCacheSource {
    pub routing_key: u64,
    pub mip_level: u32,
    pub generation: u64,
    pub tile_range: Option<TileRange>,
}

impl Producer for TileCacheSource {
    fn kind(&self) -> &'static str { "tile_cache_source" }
    fn out_ports(&self) -> &'static OutPortSpecification { &VCS_OUT_PORTS }
    fn source_items(&self) -> usize { 0 }

    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let cb = TILE_READERS.read().get(&self.routing_key).cloned()
            .ok_or_else(|| Error::internal("tile cache reader not registered"))?;
        let items = (cb)(self.routing_key, self.generation, self.mip_level, self.tile_range.clone());
        for item in items { ctx.emit.emit(item); }
        Ok(())
    }
}
