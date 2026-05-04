pub mod tile_to_tile_block;
pub mod to_neighborhood;
pub mod to_scanline;
pub mod to_tile;

pub use tile_to_tile_block::TileToTileBlock;
pub use to_neighborhood::NeighborhoodAgg;
pub use to_scanline::TileToScanline;
pub use to_tile::ScanLineAccumulator;

use serde::{Deserialize, Serialize};

use crate::data::Device;
use crate::stage::{CpuKernel, PortSpec, Stage, StageHints};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataTransformNode {
    NeighborhoodAgg(NeighborhoodAgg),
    ScanLineAccumulator(ScanLineAccumulator),
    TileToScanline(TileToScanline),
    TileToTileBlock(TileToTileBlock),
}

impl Stage for DataTransformNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::NeighborhoodAgg(s) => s.kind(),
            Self::ScanLineAccumulator(s) => s.kind(),
            Self::TileToScanline(s) => s.kind(),
            Self::TileToTileBlock(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::NeighborhoodAgg(s) => s.ports(),
            Self::ScanLineAccumulator(s) => s.ports(),
            Self::TileToScanline(s) => s.ports(),
            Self::TileToTileBlock(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::NeighborhoodAgg(s) => s.hints(),
            Self::ScanLineAccumulator(s) => s.hints(),
            Self::TileToScanline(s) => s.hints(),
            Self::TileToTileBlock(s) => s.hints(),
        }
    }

    fn device(&self) -> Device { Device::Either }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::NeighborhoodAgg(s) => s.cpu_kernel(),
            Self::ScanLineAccumulator(s) => s.cpu_kernel(),
            Self::TileToScanline(s) => s.cpu_kernel(),
            Self::TileToTileBlock(s) => s.cpu_kernel(),
        }
    }
}
