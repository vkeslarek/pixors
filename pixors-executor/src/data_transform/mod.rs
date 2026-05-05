pub mod tile_to_tile_block;
pub mod to_neighborhood;
pub mod to_scanline;
pub mod to_tile;

use serde::{Deserialize, Serialize};

use crate::delegate_stage;
use crate::data_transform::tile_to_tile_block::TileToTileBlock;
use crate::data_transform::to_neighborhood::TileToNeighborhood;
use crate::data_transform::to_scanline::TileToScanline;
use crate::data_transform::to_tile::ScanLineToTile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataTransformNode {
    TileToNeighborhood(TileToNeighborhood),
    ScanLineToTile(ScanLineToTile),
    TileToScanline(TileToScanline),
    TileToTileBlock(TileToTileBlock),
}

delegate_stage!(DataTransformNode, TileToNeighborhood, ScanLineToTile, TileToScanline, TileToTileBlock);
