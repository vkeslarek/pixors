use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::data::{Tile, TileBlock, TileBlockCoord, TileGridPos};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};
use crate::error::Error;
use crate::debug_stopwatch;

static IN: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static OUT: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static PORTS: PortSpec = PortSpec { inputs: IN, outputs: OUT };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileToTileBlock {
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
}

impl Stage for TileToTileBlock {
    fn kind(&self) -> &'static str { "tile_to_tile_block" }

    fn ports(&self) -> &'static PortSpec { &PORTS }

    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadOnly, prefers_gpu: false }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(TileToTileBlockRunner::new(
            self.tile_size, self.image_width, self.image_height,
        )))
    }
}

pub struct TileToTileBlockRunner {
    tile_size: u32,
    image_width: u32,
    image_height: u32,
    grid: HashMap<TileGridPos, Tile>,
}

impl TileToTileBlockRunner {
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

impl CpuKernel for TileToTileBlockRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("tile_to_tile_block");
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("TileToTileBlock expected Tile")),
        };

        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;

        emit.emit(Item::Tile(tile.clone()));

        let key = TileGridPos { mip_level: mip, tx, ty };
        self.grid.insert(key, tile);

        for (tx_tl, ty_tl) in Self::candidate_blocks(tx, ty) {
            if let Some(block_tiles) = self.take_block(mip, tx_tl, ty_tl) {
                let coord = TileBlockCoord { mip_level: mip, tx_tl, ty_tl };
                emit.emit(Item::TileBlock(TileBlock { coord, tiles: block_tiles }));
            }
        }

        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
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
                emit.emit(Item::TileBlock(TileBlock { coord, tiles }));
            }
        }
        Ok(())
    }
}
