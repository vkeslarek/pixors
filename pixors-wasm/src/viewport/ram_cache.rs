use crate::pixel::Rgba;
use std::collections::HashMap;

pub struct RamTileCache {
    tiles: HashMap<(u32, u32, u32), Vec<Rgba<u8>>>, // (mip, tx, ty) → pixels
}

impl RamTileCache {
    pub fn new() -> Self {
        Self { tiles: HashMap::new() }
    }

    pub fn put(&mut self, mip: u32, tx: u32, ty: u32, data: Vec<Rgba<u8>>) {
        self.tiles.insert((mip, tx, ty), data);
    }

    pub fn get(&self, mip: u32, tx: u32, ty: u32) -> Option<&Vec<Rgba<u8>>> {
        self.tiles.get(&(mip, tx, ty))
    }

    pub fn has(&self, mip: u32, tx: u32, ty: u32) -> bool {
        self.tiles.contains_key(&(mip, tx, ty))
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }
}
