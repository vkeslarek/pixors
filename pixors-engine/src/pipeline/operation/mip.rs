use crate::image::{Tile, TileCoord};
use crate::pixel::PixelAccumulator;
use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use std::collections::HashMap;
use std::sync::Arc;

/// Accumulates tiles 2×2 → downsamples → emits all MIP levels.
/// Stateful: keeps a quadrant buffer keyed by (src_mip, dst_tx, dst_ty).
/// Recursive: when a MIP tile is generated, it feeds back into process().
pub struct MipOp<P: PixelAccumulator> {
    tile_size: u32,
    max_levels: u32,
    image_width: u32,
    image_height: u32,
    accum: HashMap<(u32, u32, u32), [Option<Arc<Tile<P>>>; 4]>,
}

impl<P: PixelAccumulator> MipOp<P> {
    pub fn new(tile_size: u32, max_levels: u32, image_width: u32, image_height: u32) -> Self {
        Self { tile_size, max_levels, image_width, image_height, accum: HashMap::new() }
    }

    fn downsample_quadrant(
        entry: &[Option<Arc<Tile<P>>>; 4],
        dst_mip: u32,
        dst_tx: u32,
        dst_ty: u32,
        tile_size: u32,
        iw: u32,
        ih: u32,
    ) -> Tile<P> {
        let coord = TileCoord::new(dst_mip, dst_tx, dst_ty, tile_size, iw, ih);
        let ow = coord.width as usize;
        let oh = coord.height as usize;

        let mut data = Vec::with_capacity(ow * oh);
        for oy in 0..oh {
            for ox in 0..ow {
                let mut sum = P::Sum::default();
                let mut count = 0u32;
                let sx_base = dst_tx as usize * tile_size as usize * 2 + ox * 2;
                let sy_base = dst_ty as usize * tile_size as usize * 2 + oy * 2;

                for dy in 0..2u32 {
                    for dx in 0..2u32 {
                        let sx = sx_base + dx as usize;
                        let sy = sy_base + dy as usize;

                        // Determine which source tile this pixel belongs to.
                        let stx = sx / tile_size as usize;
                        let sty = sy / tile_size as usize;
                        let ex = stx.wrapping_sub(dst_tx as usize * 2);
                        let ey = sty.wrapping_sub(dst_ty as usize * 2);
                        if ex < 2 && ey < 2 {
                            let qi = (ey * 2 + ex) as usize;
                            if let Some(ref tile) = entry[qi] {
                                let px = tile.coord.px as usize;
                                let py = tile.coord.py as usize;
                                let tw = tile.coord.width as usize;
                                let th = tile.coord.height as usize;
                                if sx >= px && sx < px + tw && sy >= py && sy < py + th {
                                    let lx = sx - px;
                                    let ly = sy - py;
                                    tile.data[ly * tw + lx].accumulate(&mut sum);
                                    count += 1;
                                }
                            }
                        }
                    }
                }

                let c = count.max(1);
                data.push(P::from_sum(sum, c));
            }
        }

        Tile::new(coord, data)
    }
}

impl<P: PixelAccumulator> Operation for MipOp<P> {
    type In = Tile<P>;
    type Out = Tile<P>;

    fn name(&self) -> &'static str { "mip" }

    fn process(
        &mut self,
        tile: Self::In,
        emit: &mut Emitter<Self::Out>,
    ) -> Result<(), crate::error::Error> {
        let tile = Arc::new(tile);
        let src_mip = tile.coord.mip_level;

        let dlen = tile.data.len();
        let expected = (tile.coord.width * tile.coord.height) as usize;
        if dlen != expected {
            tracing::warn!(
                "[MipOp] tile data len {} != coord w={} h={} expected={} at mip={} tx={} ty={}",
                dlen, tile.coord.width, tile.coord.height, expected, src_mip, tile.coord.tx, tile.coord.ty
            );
        }

        // Always pass through (unwrap Arc since output is Tile)
        emit.emit(Tile::new(tile.coord, (*tile.data).clone()));

        if src_mip >= self.max_levels {
            return Ok(());
        }

        let dst_mip = src_mip + 1;
        let dst_tx = tile.coord.tx / 2;
        let dst_ty = tile.coord.ty / 2;
        let qi = ((tile.coord.ty % 2) * 2 + (tile.coord.tx % 2)) as usize;

        let key = (src_mip, dst_tx, dst_ty);
        let entry = self.accum.entry(key).or_insert_with(|| [None, None, None, None]);
        entry[qi] = Some(tile);

        // Check completeness: for interior tiles we need 4 quadrants,
        // for edge tiles we may need fewer.
        let src_w = self.image_width >> src_mip;
        let src_h = self.image_height >> src_mip;
        let tiles_x = src_w.div_ceil(self.tile_size);
        let tiles_y = src_h.div_ceil(self.tile_size);
        let req_w = if dst_tx * 2 + 1 < tiles_x { 2 } else { 1 };
        let req_h = if dst_ty * 2 + 1 < tiles_y { 2 } else { 1 };

        let complete = (0..req_h).all(|dy| (0..req_w).all(|dx| {
            let idx = (dy * 2 + dx) as usize;
            entry[idx].is_some()
        }));

        if complete {
            let dst_iw = (self.image_width >> dst_mip).max(1);
            let dst_ih = (self.image_height >> dst_mip).max(1);

            let new_tile = Self::downsample_quadrant(
                entry, dst_mip, dst_tx, dst_ty, self.tile_size, dst_iw, dst_ih,
            );

            // Clear entry
            *entry = [None, None, None, None];

            // Recurse: the new MIP tile also goes through process()
            self.process(new_tile, emit)?;
        }

        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Self::Out>) -> Result<(), crate::error::Error> {
        // Flush partial quadrants at image edges
        let keys: Vec<_> = self.accum.keys().cloned().collect();
        for (src_mip, dst_tx, dst_ty) in keys {
            let entry = self.accum.remove(&(src_mip, dst_tx, dst_ty));
            if let Some(entry) = entry {
                let any_some = entry.iter().any(|o| o.is_some());
                if any_some {
                    let dst_iw = (self.image_width >> (src_mip + 1)).max(1);
                    let dst_ih = (self.image_height >> (src_mip + 1)).max(1);
                    let new_tile = Self::downsample_quadrant(
                        &entry, src_mip + 1, dst_tx, dst_ty, self.tile_size, dst_iw, dst_ih,
                    );
                    emit.emit(new_tile);
                }
            }
        }
        Ok(())
    }
}
