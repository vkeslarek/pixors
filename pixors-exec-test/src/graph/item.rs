use crate::data::{Neighborhood, ScanLine, Tile};
use crate::stage::DataKind;

#[derive(Debug, Clone)]
pub enum Item {
    ScanLine(ScanLine),
    Tile(Tile),
    Neighborhood(Neighborhood),
}

impl Item {
    pub fn kind(&self) -> DataKind {
        match self {
            Item::Tile(_) => DataKind::Tile,
            Item::Neighborhood(_) => DataKind::Neighborhood,
            Item::ScanLine(_) => DataKind::ScanLine,
        }
    }
}
