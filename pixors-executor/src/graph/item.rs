use crate::data::neighborhood::Neighborhood;
use crate::data::scanline::ScanLine;
use crate::data::tile::Tile;
use crate::data::tile_block::TileBlock;
use crate::stage::DataKind;

#[derive(Debug, Clone)]
pub enum Item {
    ScanLine(ScanLine),
    Tile(Tile),
    TileBlock(TileBlock),
    Neighborhood(Neighborhood),
}

impl Item {
    pub fn kind(&self) -> DataKind {
        match self {
            Item::Tile(_) => DataKind::Tile,
            Item::TileBlock(_) => DataKind::TileBlock,
            Item::Neighborhood(_) => DataKind::Neighborhood,
            Item::ScanLine(_) => DataKind::ScanLine,
        }
    }
}
