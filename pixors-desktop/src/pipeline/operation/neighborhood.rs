use pixors_engine::image::{EdgeCondition, Neighborhood, NeighborhoodCoord, Tile, TileCoord, TileGrid};
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use std::collections::HashMap;
use std::sync::Arc;

/// Accumulates tiles per NeighborhoodCoord and emits complete Neighborhoods.
/// `pixel_radius` is at mip 0; per-mip tile radius derives from it.
pub struct NeighborhoodAccumOp<P: Clone + Send + 'static> {
    pixel_radius: u32,
    edge: EdgeCondition,
    cache: HashMap<NeighborhoodCoord, Arc<Tile<P>>>,
    emitted: HashMap<NeighborhoodCoord, bool>,
    image_width: u32,
    image_height: u32,
    tile_size: u32,
}

impl<P: Clone + Send + Sync + 'static> NeighborhoodAccumOp<P> {
    pub fn new(pixel_radius: u32, image_width: u32, image_height: u32, tile_size: u32) -> Self {
        Self { pixel_radius, edge: EdgeCondition::Clamp, cache: HashMap::new(), emitted: HashMap::new(), image_width, image_height, tile_size }
    }

    fn tile_radius_for_mip(&self, mip: u32) -> u32 {
        let pr = self.pixel_radius >> mip;
        if pr == 0 { 0 } else { pr.div_ceil(self.tile_size) }
    }

    fn try_emit(&mut self, coord: NeighborhoodCoord, emit: &mut Emitter<Neighborhood<P>>) {
        let r = self.tile_radius_for_mip(coord.mip) as i32;
        let iw = (self.image_width >> coord.mip).max(1);
        let ih = (self.image_height >> coord.mip).max(1);
        let tx_max = iw.div_ceil(self.tile_size) as i32 - 1;
        let ty_max = ih.div_ceil(self.tile_size) as i32 - 1;
        let all_present = (-r..=r).all(|dy| (-r..=r).all(|dx| {
            let gx = (coord.tx as i32 + dx).clamp(0, tx_max);
            let gy = (coord.ty as i32 + dy).clamp(0, ty_max);
            self.cache.contains_key(&NeighborhoodCoord::new(coord.mip, gx as u32, gy as u32))
        }));
        if !all_present || self.emitted.contains_key(&coord) { return; }

        let center = TileCoord::new(coord.mip, coord.tx, coord.ty, self.tile_size, iw, ih);
        let mut nbhd = Neighborhood::new(center, r as u32, iw, ih, self.tile_size, self.edge);
        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (coord.tx as i32 + dx).max(0) as u32;
                let gy = (coord.ty as i32 + dy).max(0) as u32;
                nbhd.insert((dx, dy), self.cache.get(&NeighborhoodCoord::new(coord.mip, gx, gy)).cloned());
            }
        }
        self.emitted.insert(coord, true);
        emit.emit(nbhd);
    }
}

impl<P: Clone + Send + Sync + 'static> Operation for NeighborhoodAccumOp<P> {
    type In = Tile<P>;
    type Out = Neighborhood<P>;

    fn name(&self) -> &'static str { "neighborhood" }

    fn process(&mut self, tile: Arc<Self::In>, emit: &mut Emitter<Self::Out>) -> Result<(), pixors_engine::error::Error> {
        let ncoord = NeighborhoodCoord::from_tile(&tile.coord);
        self.cache.insert(ncoord, Arc::clone(&tile));

        let iw = (self.image_width >> ncoord.mip).max(1);
        let ih = (self.image_height >> ncoord.mip).max(1);
        let r = self.tile_radius_for_mip(ncoord.mip) as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let gx = (ncoord.tx as i32 + dx).max(0) as u32;
                let gy = (ncoord.ty as i32 + dy).max(0) as u32;
                let c = NeighborhoodCoord::new(ncoord.mip, gx, gy);
                if c.tx * self.tile_size < iw && c.ty * self.tile_size < ih {
                    self.try_emit(c, emit);
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Self::Out>) -> Result<(), pixors_engine::error::Error> {
        let mut mips: Vec<u32> = self.cache.keys().map(|c| c.mip).collect();
        mips.sort(); mips.dedup();
        for mip in mips {
            let iw = (self.image_width >> mip).max(1);
            let ih = (self.image_height >> mip).max(1);
            let grid = TileGrid::new(iw, ih, self.tile_size);
            for coord in grid.tiles() {
                let ncoord = NeighborhoodCoord::new(mip, coord.tx, coord.ty);
                if self.emitted.contains_key(&ncoord) { continue; }
                if self.cache.contains_key(&ncoord) { self.try_emit(ncoord, emit); }
            }
        }
        Ok(())
    }
}
