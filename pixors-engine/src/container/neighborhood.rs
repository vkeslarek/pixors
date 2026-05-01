use crate::container::meta::PixelMeta;
use crate::container::tile::TileCoord;
use crate::container::Container;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NeighborhoodCoord {
    pub tx: u32,
    pub ty: u32,
}

impl NeighborhoodCoord {
    pub fn new(tx: u32, ty: u32) -> Self {
        Self { tx, ty }
    }

    pub fn from_tile(tile: &TileCoord) -> Self {
        Self {
            tx: tile.tx,
            ty: tile.ty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeCondition {
    Clamp,
    Mirror,
    Transparent,
}

#[derive(Debug, Clone)]
pub struct Neighborhood {
    pub radius: u32,
    pub center: TileCoord,
    pub tiles: Vec<TileCoord>,
    pub edge: EdgeCondition,
    pub meta: PixelMeta,
}

impl Neighborhood {
    pub fn new(
        radius: u32,
        center: TileCoord,
        tiles: Vec<TileCoord>,
        edge: EdgeCondition,
        meta: PixelMeta,
    ) -> Self {
        Self {
            radius,
            center,
            tiles,
            edge,
            meta,
        }
    }
}

impl Container for Neighborhood {
    fn meta(&self) -> &PixelMeta {
        &self.meta
    }
}
