use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use serde::{Deserialize, Serialize};

use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_ops::source::cache_reader::TileRange;
use pixors_engine::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, ProcessorContext, Producer, Stage,
};

pub type TileReadFn = Box<dyn Fn(u64, u64, u32, Option<TileRange>) -> Vec<Item> + Send + Sync>;
//                               key, gen,  mip, range

static TILE_READERS: LazyLock<RwLock<HashMap<u64, Arc<TileReadFn>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Register a per-tab tile reader keyed by `tab_id`.
pub fn install_tile_cache_reader(tab_id: u64, f: TileReadFn) {
    TILE_READERS.write().unwrap().insert(tab_id, Arc::new(f));
}

/// Check if a tile reader is installed for the given `tab_id`.
pub fn is_tile_cache_reader_installed(tab_id: u64) -> bool {
    TILE_READERS.read().unwrap().contains_key(&tab_id)
}

/// Remove the tile reader for `tab_id`.
pub fn uninstall_tile_cache_reader(tab_id: u64) {
    TILE_READERS.write().unwrap().remove(&tab_id);
}

static VCS_INPUTS: &[PortDeclaration] = &[];
static VCS_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static VCS_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(VCS_INPUTS),
    outputs: PortGroup::Fixed(VCS_OUTPUTS),
};

/// Reads tiles from an in-memory cache installed by the desktop layer.
/// The desktop registers a global callback keyed by `routing_key` (TabId.0)
/// so multiple tabs can use separate ViewportCaches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileCacheSource {
    pub routing_key: u64,
    pub mip_level: u32,
    pub generation: u64,
    pub tile_range: Option<TileRange>,
}

impl Stage for TileCacheSource {
    fn kind(&self) -> &'static str {
        "tile_cache_source"
    }

    fn ports(&self) -> &'static PortSpecification {
        &VCS_PORTS
    }

    fn producer(&self) -> Option<Box<dyn Producer>> {
        let cb = TILE_READERS
            .read()
            .unwrap()
            .get(&self.routing_key)
            .cloned()?;
        Some(Box::new(TileCacheSourceProducer {
            cb,
            routing_key: self.routing_key,
            mip_level: self.mip_level,
            generation: self.generation,
            tile_range: self.tile_range.clone(),
        }))
    }

    fn source_items(&self) -> usize {
        0
    }
}

pub struct TileCacheSourceProducer {
    cb: Arc<TileReadFn>,
    routing_key: u64,
    mip_level: u32,
    generation: u64,
    tile_range: Option<TileRange>,
}

impl Producer for TileCacheSourceProducer {
    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let items = (self.cb)(
            self.routing_key,
            self.generation,
            self.mip_level,
            self.tile_range.clone(),
        );
        for item in items {
            ctx.emit.emit(item);
        }
        Ok(())
    }
}
