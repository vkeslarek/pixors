use crate::data::tile::Tile;
use crate::data::tile::TileCoord;
use crate::model::pixel::meta::PixelMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NeighborhoodCoord {
    pub mip_level: u32,
    pub tx: u32,
    pub ty: u32,
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
    pub tiles: Vec<Tile>,
    pub edge: EdgeCondition,
    pub meta: PixelMeta,
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
}

impl Neighborhood {
    pub fn new(
        radius: u32,
        center: TileCoord,
        tiles: Vec<Tile>,
        edge: EdgeCondition,
        meta: PixelMeta,
        image_width: u32,
        image_height: u32,
        tile_size: u32,
    ) -> Self {
        Self {
            radius,
            center,
            tiles,
            edge,
            meta,
            image_width,
            image_height,
            tile_size,
        }
    }

    pub fn tile_at(&self, tx: u32, ty: u32) -> Option<&Tile> {
        self.tiles
            .iter()
            .find(|t| t.coord.tx == tx && t.coord.ty == ty)
    }
}
