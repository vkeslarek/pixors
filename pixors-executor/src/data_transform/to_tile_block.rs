use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use crate::data::device::Device;
use crate::data::tile::{Tile, TileGridPos};
use crate::data::tile_block::{TileBlock, TileBlockCoord};
use crate::graph::item::Item;
use crate::stage::{BufferAccess, Processor, ProcessorContext, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage, StageHints};

use crate::error::Error;

use crate::debug_stopwatch;


static IN: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static OUT: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static PORTS: PortSpecification = PortSpecification { inputs: PortGroup::Fixed(IN), outputs: PortGroup::Fixed(OUT) };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileToTileBlock {
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
}

impl Stage for TileToTileBlock {
    fn kind(&self) -> &'static str { "tile_to_tile_block" }

    fn ports(&self) -> &'static PortSpecification { &PORTS }

    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadOnly, prefers_gpu: false }
    }

    fn device(&self) -> Device { Device::Either }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(TileToTileBlockProcessor::new(
            self.tile_size, self.image_width, self.image_height,
        )))
    }
}

pub struct TileToTileBlockProcessor {
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    grid: HashMap<TileGridPos, Tile>,
}

impl TileToTileBlockProcessor {
    pub fn new(tile_size: u32, image_width: u32, image_height: u32) -> Self {
        Self { tile_size, image_width, image_height, grid: HashMap::new() }
    }

    fn candidate_blocks(tx: u32, ty: u32) -> Vec<(u32, u32)> {
        let mut candidates = Vec::with_capacity(4);
        for dy in [0u32, 1].iter().copied() {
            if ty < dy { continue; }
            for dx in [0u32, 1].iter().copied() {
                if tx < dx { continue; }
                let tx_tl = tx - dx;
                let ty_tl = ty - dy;
                if tx_tl % 2 == 0 && ty_tl % 2 == 0 {
                    candidates.push((tx_tl, ty_tl));
                }
            }
        }
        candidates
    }

    fn is_block_ready(&self, mip: u32, tx_tl: u32, ty_tl: u32) -> bool {
        let keys = [
            TileGridPos { mip_level: mip, tx: tx_tl, ty: ty_tl },
            TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl },
            TileGridPos { mip_level: mip, tx: tx_tl, ty: ty_tl + 1 },
            TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl + 1 },
        ];
        keys.iter().all(|k| self.grid.contains_key(k))
    }

    fn take_block(&mut self, mip: u32, tx_tl: u32, ty_tl: u32) -> Option<[Tile; 4]> {
        if !self.is_block_ready(mip, tx_tl, ty_tl) {
            return None;
        }
        let k00 = TileGridPos { mip_level: mip, tx: tx_tl, ty: ty_tl };
        let k01 = TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl };
        let k10 = TileGridPos { mip_level: mip, tx: tx_tl, ty: ty_tl + 1 };
        let k11 = TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl + 1 };
        Some([
            self.grid.remove(&k00).unwrap(),
            self.grid.remove(&k01).unwrap(),
            self.grid.remove(&k10).unwrap(),
            self.grid.remove(&k11).unwrap(),
        ])
    }
}

impl Processor for TileToTileBlockProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("tile_to_tile_block");
        let tile = ProcessorContext::take_tile(item)?;

        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;

        ctx.emit.emit(Item::Tile(tile.clone()));

        let key = TileGridPos { mip_level: mip, tx, ty };
        self.grid.insert(key, tile);

        for (tx_tl, ty_tl) in Self::candidate_blocks(tx, ty) {
            if let Some(block_tiles) = self.take_block(mip, tx_tl, ty_tl) {
                let coord = TileBlockCoord { mip_level: mip, tx_tl, ty_tl };
                ctx.emit.emit(Item::TileBlock(TileBlock { coord, tiles: block_tiles }));
            }
        }

        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let remaining: Vec<(u32, u32, u32)> = self.grid.keys()
            .map(|k| (k.mip_level, k.tx, k.ty))
            .collect();
        for (mip, tx, ty) in remaining {
            // Try to form partial blocks (odd image dimensions)
            let tx_tl = (tx / 2) * 2;
            let ty_tl = (ty / 2) * 2;
            let mut partial = [
                self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl, ty: ty_tl }),
                self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl }),
                self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl, ty: ty_tl + 1 }),
                self.grid.remove(&TileGridPos { mip_level: mip, tx: tx_tl + 1, ty: ty_tl + 1 }),
            ];
            let count = partial.iter().filter(|o| o.is_some()).count();
            if count > 0 && count < 4 {
                // Fill missing slots with copies of existing tiles (clamp)
                let filler = partial.iter().find(|o| o.is_some()).unwrap().clone().unwrap();
                for slot in partial.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(filler.clone());
                    }
                }
                let tiles: [Tile; 4] = [
                    partial[0].take().unwrap(),
                    partial[1].take().unwrap(),
                    partial[2].take().unwrap(),
                    partial[3].take().unwrap(),
                ];
                let coord = TileBlockCoord { mip_level: mip, tx_tl, ty_tl };
                ctx.emit.emit(Item::TileBlock(TileBlock { coord, tiles }));
            }
        }
        Ok(())
    }
}
