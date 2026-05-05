use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::data::tile::{Tile, TileGridPos};
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::model::image::desc::BlendMode;
use crate::stage::{
    BufferAccess, DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage, StageHints,
};

static COMPOSE_INPUT: PortDeclaration = PortDeclaration {
    name: "layers",
    kind: DataKind::Tile,
};
static COMPOSE_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "composed",
    kind: DataKind::Tile,
}];
static COMPOSE_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Variable(&COMPOSE_INPUT),
    outputs: PortGroup::Fixed(COMPOSE_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compose {
    pub layer_count: u16,
    pub blend_modes: Vec<BlendMode>,
}

impl Stage for Compose {
    fn kind(&self) -> &'static str {
        "compose"
    }

    fn ports(&self) -> &'static PortSpecification {
        &COMPOSE_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(ComposeProcessor::new(
            self.layer_count,
            self.blend_modes.clone(),
        )))
    }
}

pub struct ComposeProcessor {
    layer_count: u16,
    blend_modes: Vec<BlendMode>,
    grid: HashMap<TileGridPos, Vec<Option<Tile>>>,
}

impl ComposeProcessor {
    pub fn new(layer_count: u16, blend_modes: Vec<BlendMode>) -> Self {
        Self {
            layer_count,
            blend_modes,
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

        compose_and_emit(tiles, &self.blend_modes, emit);
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
        compose_and_emit(tiles, &self.blend_modes, emit);
    }
}

impl Processor for ComposeProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let tile = ProcessorContext::take_tile(item)?;

        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;
        let key = TileGridPos {
            mip_level: mip,
            tx,
            ty,
        };

        if ctx.port >= self.layer_count {
            return Err(Error::internal(format!(
                "Compose: port {} out of bounds (layer_count {})",
                ctx.port, self.layer_count,
            )));
        }
        let slots = self
            .grid
            .entry(key)
            .or_insert_with(|| vec![None; self.layer_count as usize]);
        slots[ctx.port as usize] = Some(tile);

        self.try_compose(mip, tx, ty, ctx.emit);
        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let keys: Vec<TileGridPos> = self.grid.keys().cloned().collect();
        for key in keys {
            self.flush_slot(key, ctx.emit);
        }
        Ok(())
    }
}

fn compose_and_emit(tiles: Vec<(u16, Tile)>, blend_modes: &[BlendMode], emit: &mut Emitter<Item>) {
    if tiles.is_empty() {
        return;
    }

    let bpp = tiles[0].1.meta.format.bytes_per_pixel() as usize;
    let mut w = 0;
    let mut h = 0;
    for (_, tile) in tiles.iter() {
        w = w.max(tile.coord.width as usize);
        h = h.max(tile.coord.height as usize);
    }
    let meta = tiles[0].1.meta;
    let mut coord = tiles[0].1.coord;
    coord.width = w as u32;
    coord.height = h as u32;

    let mut out = vec![0u8; w * h * bpp];

    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * bpp;
            let mut result: [u8; 4] = [0, 0, 0, 0];
            let mut started = false;

            for (port, tile) in tiles.iter() {
                if x >= tile.coord.width as usize || y >= tile.coord.height as usize {
                    continue;
                }

                let data = match &tile.data {
                    Buffer::Cpu(v) => v.as_slice(),
                    Buffer::Gpu(_) => continue,
                };
                let t_off = (y * tile.coord.width as usize + x) * bpp;
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
                    let mode = blend_modes.get(*port as usize).copied().unwrap_or_default();
                    result = blend(&src, &result, mode);
                }
            }

            out[off..off + bpp].copy_from_slice(&result);
        }
    }

    emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(out))));
}

fn blend(top: &[u8; 4], bottom: &[u8; 4], mode: BlendMode) -> [u8; 4] {
    match mode {
        BlendMode::Normal => alpha_over(top, bottom),
    }
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
