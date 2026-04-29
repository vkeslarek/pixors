use crate::image::{EdgeCondition, Neighborhood, Tile, TileCoord, TileGrid};
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use std::collections::HashMap;
use std::sync::Arc;

/// Accumulates tiles and emits complete Neighborhoods.
/// Each incoming tile gets buffered; when all neighbors within `radius` are
/// available, the neighborhood for that tile is emitted.
/// Remaining tiles are flushed in `finish()` with edge clamping.
pub struct NeighborhoodAccumOp<P: Clone + Send + 'static> {
    radius: u32,
    edge: EdgeCondition,
    cache: HashMap<(u32, u32), Arc<Tile<P>>>,
    emitted: HashMap<(u32, u32), bool>,
    image_width: u32,
    image_height: u32,
    tile_size: u32,
}

impl<P: Clone + Send + Sync + 'static> NeighborhoodAccumOp<P> {
    pub fn new(radius: u32, image_width: u32, image_height: u32, tile_size: u32) -> Self {
        Self {
            radius,
            edge: EdgeCondition::Clamp,
            cache: HashMap::new(),
            emitted: HashMap::new(),
            image_width,
            image_height,
            tile_size,
        }
    }

    pub fn with_edge(mut self, edge: EdgeCondition) -> Self {
        self.edge = edge;
        self
    }

    fn try_emit(&mut self, tx: u32, ty: u32, emit: &mut Emitter<Neighborhood<P>>) {
        let r = self.radius as i32;
        let all_present = (-r..=r).all(|dy| {
            (-r..=r).all(|dx| {
                let gx = (tx as i32 + dx).clamp(0, self.image_width as i32 - 1);
                let gy = (ty as i32 + dy).clamp(0, self.image_height as i32 - 1);
                self.cache.contains_key(&(gx as u32, gy as u32))
            })
        });

        if !all_present {
            return;
        }

        if self.emitted.contains_key(&(tx, ty)) {
            return;
        }

        let center = TileCoord::new(0, tx, ty, self.tile_size, self.image_width, self.image_height);
        let mut nbhd = Neighborhood::new(
            center,
            self.radius,
            self.image_width,
            self.image_height,
            self.tile_size,
            self.edge,
        );

        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (tx as i32 + dx).max(0) as u32;
                let gy = (ty as i32 + dy).max(0) as u32;
                nbhd.insert((dx, dy), self.cache.get(&(gx, gy)).cloned());
            }
        }

        self.emitted.insert((tx, ty), true);
        emit.emit(nbhd);
    }
}

impl<P: Clone + Send + Sync + 'static> Operation for NeighborhoodAccumOp<P> {
    type In = Arc<Tile<P>>;
    type Out = Neighborhood<P>;

    fn name(&self) -> &'static str {
        "neighborhood_accum"
    }

    fn process(
        &mut self,
        tile: Self::In,
        emit: &mut Emitter<Self::Out>,
    ) -> Result<(), crate::error::Error> {
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;
        self.cache.insert((tx, ty), tile);

        // Try to emit neighborhoods for this tile and all nearby tiles
        let r = self.radius as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (tx as i32 + dx).max(0) as u32;
                let gy = (ty as i32 + dy).max(0) as u32;
                if gx * self.tile_size < self.image_width
                    && gy * self.tile_size < self.image_height
                {
                    self.try_emit(gx, gy, emit);
                }
            }
        }

        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Self::Out>) -> Result<(), crate::error::Error> {
        let grid = TileGrid::new(self.image_width, self.image_height, self.tile_size);
        for coord in grid.tiles() {
            let tx = coord.tx;
            let ty = coord.ty;
            if self.emitted.contains_key(&(tx, ty)) {
                continue;
            }
            if self.cache.contains_key(&(tx, ty)) {
                self.try_emit(tx, ty, emit);
            }
        }
        Ok(())
    }
}
