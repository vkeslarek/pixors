use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::data::{Buffer, Tile, TileGridPos};
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, CpuKernel, DataKind, PortDecl, PortGroup, PortSpec, Stage, StageHints,
};

static COMPOSE_INPUT: PortDecl = PortDecl {
    name: "layers",
    kind: DataKind::Tile,
};
static COMPOSE_OUTPUTS: &[PortDecl] = &[PortDecl {
    name: "composed",
    kind: DataKind::Tile,
}];
static COMPOSE_PORTS: PortSpec = PortSpec {
    inputs: PortGroup::Variable(&COMPOSE_INPUT),
    outputs: PortGroup::Fixed(COMPOSE_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compose {
    pub layer_count: u16,
}

impl Stage for Compose {
    fn kind(&self) -> &'static str {
        "compose"
    }

    fn ports(&self) -> &'static PortSpec {
        &COMPOSE_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(ComposeRunner::new(self.layer_count)))
    }
}

pub struct ComposeRunner {
    layer_count: u16,
    grid: HashMap<TileGridPos, Vec<Option<Tile>>>,
}

impl ComposeRunner {
    pub fn new(layer_count: u16) -> Self {
        Self {
            layer_count,
            grid: HashMap::new(),
        }
    }

    fn try_compose(&mut self, mip: u32, tx: u32, ty: u32, emit: &mut Emitter<Item>) {
        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };
        let Some(slots) = self.grid.get(&key) else {
            return;
        };
        if slots.iter().any(|s| s.is_none()) {
            return;
        }

        let tiles: Vec<(u16, Tile)> = self
            .grid
            .remove(&key)
            .unwrap()
            .into_iter()
            .enumerate()
            .filter_map(|(i, o)| o.map(|t| (i as u16, t)))
            .collect();

        compose_and_emit(tiles, emit);
    }

    fn flush_slot(&mut self, key: TileGridPos, emit: &mut Emitter<Item>) {
        let Some(slots) = self.grid.remove(&key) else {
            return;
        };
        let tiles: Vec<(u16, Tile)> = slots
            .into_iter()
            .enumerate()
            .filter_map(|(i, o)| o.map(|t| (i as u16, t)))
            .collect();
        compose_and_emit(tiles, emit);
    }
}

impl CpuKernel for ComposeRunner {
    fn process(
        &mut self,
        port: u16,
        item: Item,
        emit: &mut Emitter<Item>,
    ) -> Result<(), Error> {
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("Compose expected Tile")),
        };

        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;
        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };

        let slots = self
            .grid
            .entry(key)
            .or_insert_with(|| vec![None; self.layer_count as usize]);
        slots[port as usize] = Some(tile);

        self.try_compose(mip, tx, ty, emit);
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let keys: Vec<TileGridPos> = self.grid.keys().cloned().collect();
        for key in keys {
            self.flush_slot(key, emit);
        }
        Ok(())
    }
}

fn compose_and_emit(tiles: Vec<(u16, Tile)>, emit: &mut Emitter<Item>) {
    if tiles.is_empty() {
        return;
    }

    let bpp = tiles[0].1.meta.format.bytes_per_pixel() as usize;
    let w = tiles[0].1.coord.width as usize;
    let h = tiles[0].1.coord.height as usize;
    let meta = tiles[0].1.meta;
    let coord = tiles[0].1.coord;

    let mut out = vec![0u8; w * h * bpp];

    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * bpp;
            let mut result: [u8; 4] = [0, 0, 0, 0];
            let mut started = false;

            for (_, tile) in tiles.iter() {
                let data = match &tile.data {
                    Buffer::Cpu(v) => v.as_slice(),
                    Buffer::Gpu(_) => continue,
                };
                let px = x.min(tile.coord.width as usize - 1);
                let py = y.min(tile.coord.height as usize - 1);
                let t_off = (py * tile.coord.width as usize + px) * bpp;
                if t_off + bpp > data.len() {
                    continue;
                }
                let src: [u8; 4] = [
                    data[t_off],
                    data[t_off + 1],
                    data[t_off + 2],
                    data[t_off + 3],
                ];
                if !started {
                    result = src;
                    started = true;
                } else {
                    // Lower port = on top. Ascending order → later items go OVER earlier.
                    result = alpha_over(&result, &src);
                }
            }

            out[off..off + bpp].copy_from_slice(&result);
        }
    }

    emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(out))));
}

fn alpha_over(top: &[u8; 4], bottom: &[u8; 4]) -> [u8; 4] {
    let rt = top[0] as u32;
    let gt = top[1] as u32;
    let bt = top[2] as u32;
    let at = top[3] as u32;

    let rb = bottom[0] as u32;
    let gb = bottom[1] as u32;
    let bb = bottom[2] as u32;
    let ab = bottom[3] as u32;

    let pt_r = rt * at / 255;
    let pt_g = gt * at / 255;
    let pt_b = bt * at / 255;

    let pb_r = rb * ab / 255;
    let pb_g = gb * ab / 255;
    let pb_b = bb * ab / 255;

    let inv_at = 255 - at;

    let a_result = at + ab * inv_at / 255;
    if a_result == 0 {
        return [0; 4];
    }

    let p_result_r = pt_r + pb_r * inv_at / 255;
    let p_result_g = pt_g + pb_g * inv_at / 255;
    let p_result_b = pt_b + pb_b * inv_at / 255;

    let r_result = p_result_r * 255 / a_result;
    let g_result = p_result_g * 255 / a_result;
    let b_result = p_result_b * 255 / a_result;

    [
        r_result as u8,
        g_result as u8,
        b_result as u8,
        a_result as u8,
    ]
}
