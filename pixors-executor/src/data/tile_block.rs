use crate::data::tile::Tile;

/// Coordinates of a 2×2 block within the tile grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileBlockCoord {
    pub mip_level: u32,
    pub tx_tl: u32,
    pub ty_tl: u32,
}

/// A complete 2×2 block of tiles at a given MIP level, ready for downsampling.
/// Tiles are stored in row-major order:
///   [top-left, top-right, bottom-left, bottom-right]
#[derive(Debug, Clone)]
pub struct TileBlock {
    pub coord: TileBlockCoord,
    pub tiles: [Tile; 4],
}
