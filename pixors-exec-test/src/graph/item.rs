use crate::data::{Neighborhood, ScanLine, Tile};

#[derive(Debug, Clone)]
pub enum Item {
    ScanLine(ScanLine),
    Tile(Tile),
    Neighborhood(Neighborhood),
}
