use crate::container::{Neighborhood, ScanLine, Tile};

#[derive(Debug, Clone)]
pub enum Item {
    ScanLine(ScanLine),
    Tile(Tile),
    Neighborhood(Neighborhood),
}
