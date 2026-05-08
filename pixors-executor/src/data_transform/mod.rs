pub mod to_neighborhood;
pub mod to_scanline;
pub mod to_tile;
pub mod to_tile_block;

use serde::{Deserialize, Serialize};

use crate::data_transform::to_neighborhood::TileToNeighborhood;
use crate::data_transform::to_scanline::TileToScanline;
use crate::data_transform::to_tile::ScanLineToTile;
use crate::data_transform::to_tile_block::TileToTileBlock;
use crate::delegate_stage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataTransformNode {
    TileToNeighborhood(TileToNeighborhood),
    ScanLineToTile(ScanLineToTile),
    TileToScanline(TileToScanline),
    TileToTileBlock(TileToTileBlock),
}

delegate_stage!(
    DataTransformNode,
    TileToNeighborhood,
    ScanLineToTile,
    TileToScanline,
    TileToTileBlock
);

impl From<TileToNeighborhood> for DataTransformNode {
    fn from(v: TileToNeighborhood) -> Self {
        Self::TileToNeighborhood(v)
    }
}

impl From<ScanLineToTile> for DataTransformNode {
    fn from(v: ScanLineToTile) -> Self {
        Self::ScanLineToTile(v)
    }
}

impl From<TileToScanline> for DataTransformNode {
    fn from(v: TileToScanline) -> Self {
        Self::TileToScanline(v)
    }
}

impl From<TileToTileBlock> for DataTransformNode {
    fn from(v: TileToTileBlock) -> Self {
        Self::TileToTileBlock(v)
    }
}
