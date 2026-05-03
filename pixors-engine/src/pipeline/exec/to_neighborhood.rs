use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::container::{EdgeCondition, Neighborhood, Tile};
use crate::pipeline::egraph::emitter::Emitter;
use crate::pipeline::egraph::item::Item;
use crate::pipeline::egraph::runner::OperationRunner;
use crate::pipeline::egraph::stage::{Device, Stage};
use crate::error::Error;
use crate::debug_stopwatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborhoodAgg {
    pub radius: u32,
}

impl Stage for NeighborhoodAgg {
    fn kind(&self) -> &'static str { "neighborhood_agg" }
    fn device(&self) -> Device { Device::Cpu }
    fn allocates_output(&self) -> bool { true }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(NeighborhoodAggRunner::new(self.radius)))
    }
}

pub struct NeighborhoodAggRunner {
    pixel_radius: u32,
    tile_cache: HashMap<(u32, u32), Tile>,
    emitted: HashSet<(u32, u32)>,
    image_width: u32,
    image_height: u32,
    tile_size: u32,
    meta: Option<crate::container::meta::PixelMeta>,
    initialized: bool,
}

impl NeighborhoodAggRunner {
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

    fn try_emit(&mut self, tx: u32, ty: u32, emit: &mut Emitter<Item>) {
        if self.emitted.contains(&(tx, ty)) { return; }
        let r = self.tile_radius() as i32;
        let tiles_x = self.image_width.div_ceil(self.tile_size) as i32;
        let tiles_y = self.image_height.div_ceil(self.tile_size) as i32;

        let mut nbhd_tiles = Vec::new();
        let mut center_coord = None;
        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (tx as i32 + dx).clamp(0, tiles_x - 1).max(0) as u32;
                let gy = (ty as i32 + dy).clamp(0, tiles_y - 1).max(0) as u32;
                match self.tile_cache.get(&(gx, gy)) {
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
        self.emitted.insert((tx, ty));
        emit.emit(Item::Neighborhood(nbhd));
    }
}

impl OperationRunner for NeighborhoodAggRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("neighborhood_agg");
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("expected Tile")),
        };
        self.discover_bounds(&tile);
        self.update_bounds(&tile);

        let cur_ty = tile.coord.ty;
        self.tile_cache.insert((tile.coord.tx, cur_ty), tile);

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
        let candidates: Vec<(u32, u32)> = self
            .tile_cache
            .keys()
            .copied()
            .filter(|(_, ty)| *ty <= safe_until)
            .collect();
        for (tx, ty) in candidates {
            self.try_emit(tx, ty, emit);
        }
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        // All tiles are now in cache and `image_height` is final, so the
        // last `tile_radius` bands can be emitted with correct clamping.
        let keys: Vec<(u32, u32)> = self.tile_cache.keys().copied().collect();
        for (tx, ty) in keys {
            if !self.emitted.contains(&(tx, ty)) {
                self.try_emit(tx, ty, emit);
            }
        }
        Ok(())
    }
}
