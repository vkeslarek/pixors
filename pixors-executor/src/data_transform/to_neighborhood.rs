use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use crate::data::neighborhood::{EdgeCondition, Neighborhood};
use crate::data::tile::{Tile, TileGridPos};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, Processor, DataKind, PortDeclaration, PortGroup, PortSpec, Stage, StageHints};

use crate::error::Error;

use crate::debug_stopwatch;


static NA_INPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static NA_OUTPUTS: &[PortDeclaration] = &[PortDeclaration { name: "neighborhood", kind: DataKind::Neighborhood }];

static NA_PORTS: PortSpec = PortSpec { inputs: PortGroup::Fixed(NA_INPUTS), outputs: PortGroup::Fixed(NA_OUTPUTS) };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileToNeighborhood {
    pub radius: u32,
}

impl Stage for TileToNeighborhood {
    fn kind(&self) -> &'static str { "neighborhood_agg" }

    fn ports(&self) -> &'static PortSpec {
        &NA_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(TileToNeighborhoodProcessor::new(self.radius)))
    }
}

pub struct TileToNeighborhoodProcessor {
    pixel_radius: u32,
    tile_cache: HashMap<TileGridPos, Tile>,
    emitted: HashSet<TileGridPos>,
    image_width: u32,
    image_height: u32,
    tile_size: u32,
    meta: Option<crate::model::pixel::meta::PixelMeta>,
    initialized: bool,
}

impl TileToNeighborhoodProcessor {
    pub fn new(pixel_radius: u32) -> Self {
        Self {
            pixel_radius,
            tile_cache: HashMap::new(),
            emitted: HashSet::new(),
            image_width: 0,
            image_height: 0,
            tile_size: 0,
            meta: None,
            initialized: false,
        }
    }

    fn tile_radius(&self) -> u32 {
        if self.tile_size == 0 { return 0; }
        self.pixel_radius.div_ceil(self.tile_size)
    }

    fn discover_bounds(&mut self, tile: &Tile) {
        if self.initialized { return; }
        self.meta = Some(tile.meta);
        self.tile_size = tile.coord.width.max(tile.coord.height);
        self.initialized = true;
    }

    fn update_bounds(&mut self, tile: &Tile) {
        let right = tile.coord.px + tile.coord.width;
        let bottom = tile.coord.py + tile.coord.height;
        self.image_width = self.image_width.max(right);
        self.image_height = self.image_height.max(bottom);
    }

    fn try_emit(&mut self, mip: u32, tx: u32, ty: u32, emit: &mut Emitter<Item>) {
        let key = TileGridPos { mip_level: mip, tx, ty };
        if self.emitted.contains(&key) { return; }
        let r = self.tile_radius() as i32;
        let tiles_x = self.image_width.div_ceil(self.tile_size) as i32;
        let tiles_y = self.image_height.div_ceil(self.tile_size) as i32;

        let mut nbhd_tiles = Vec::new();
        let mut center_coord = None;
        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (tx as i32 + dx).clamp(0, tiles_x - 1).max(0) as u32;
                let gy = (ty as i32 + dy).clamp(0, tiles_y - 1).max(0) as u32;
                let nkey = TileGridPos { mip_level: mip, tx: gx, ty: gy };
                match self.tile_cache.get(&nkey) {
                    Some(tile) => {
                        if dx == 0 && dy == 0 {
                            center_coord = Some(tile.coord);
                        }
                        nbhd_tiles.push(tile.clone());
                    }
                    None => { return; }
                }
            }
        }

        let center = center_coord.unwrap_or_else(|| {
            nbhd_tiles.iter()
                .find(|t| t.coord.tx == tx && t.coord.ty == ty)
                .map(|t| t.coord)
                .unwrap_or(nbhd_tiles[0].coord)
        });

        let nbhd = Neighborhood::new(
            self.pixel_radius,
            center,
            nbhd_tiles,
            EdgeCondition::Clamp,
            self.meta.unwrap(),
            self.image_width,
            self.image_height,
            self.tile_size,
        );
        self.emitted.insert(key);
        emit.emit(Item::Neighborhood(nbhd));
    }
}

impl Processor for TileToNeighborhoodProcessor {
    fn process(&mut self, _port: u16, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("neighborhood_agg");
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("expected Tile")),
        };
        self.discover_bounds(&tile);
        self.update_bounds(&tile);

        let cur_ty = tile.coord.ty;
        let pos = TileGridPos { mip_level: tile.coord.mip_level, tx: tile.coord.tx, ty: cur_ty };
        self.tile_cache.insert(pos, tile);

        // Tiles whose ty <= cur_ty - tile_radius have all of their south
        // neighbours already in the cache (we've now seen at least one tile
        // per band up to `cur_ty`). Tiles closer to the leading edge stay
        // pending — emitting them now would use a stale `image_height` and
        // miss the bands that haven't streamed in yet, producing visible
        // discontinuities along band boundaries.
        let r = self.tile_radius();
        if cur_ty < r {
            return Ok(());
        }
        let safe_until = cur_ty - r;
        let candidates: Vec<TileGridPos> = self
            .tile_cache
            .keys()
            .copied()
            .filter(|p| p.ty <= safe_until)
            .collect();
        for pos in candidates {
            self.try_emit(pos.mip_level, pos.tx, pos.ty, emit);
        }
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let keys: Vec<TileGridPos> = self.tile_cache.keys().copied().collect();
        for pos in keys {
            if !self.emitted.contains(&pos) {
                self.try_emit(pos.mip_level, pos.tx, pos.ty, emit);
            }
        }
        Ok(())
    }
}
