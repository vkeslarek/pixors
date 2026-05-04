pub mod to_neighborhood;
pub mod to_scanline;
pub mod to_tile;
pub mod to_tile_block;

pub use to_neighborhood::NeighborhoodAgg;
pub use to_scanline::TileToScanline;
pub use to_tile::ScanLineAccumulator;
pub use to_tile_block::TileBlockToTile;

use serde::{Deserialize, Serialize};

use crate::data::Device;
use crate::stage::{CpuKernel, PortSpec, Stage, StageHints};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataTransformNode {
    NeighborhoodAgg(NeighborhoodAgg),
    ScanLineAccumulator(ScanLineAccumulator),
    TileToScanline(TileToScanline),
    TileBlockToTile(TileBlockToTile),
}

impl Stage for DataTransformNode {
    fn kind(&self) -> &'static str {
        match self {
            Self::NeighborhoodAgg(s) => s.kind(),
            Self::ScanLineAccumulator(s) => s.kind(),
            Self::TileToScanline(s) => s.kind(),
            Self::TileBlockToTile(s) => s.kind(),
        }
    }

    fn ports(&self) -> &'static PortSpec {
        match self {
            Self::NeighborhoodAgg(s) => s.ports(),
            Self::ScanLineAccumulator(s) => s.ports(),
            Self::TileToScanline(s) => s.ports(),
            Self::TileBlockToTile(s) => s.ports(),
        }
    }

    fn hints(&self) -> StageHints {
        match self {
            Self::NeighborhoodAgg(s) => s.hints(),
            Self::ScanLineAccumulator(s) => s.hints(),
            Self::TileToScanline(s) => s.hints(),
            Self::TileBlockToTile(s) => s.hints(),
        }
    }

    fn device(&self) -> Device { Device::Either }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        match self {
            Self::NeighborhoodAgg(s) => s.cpu_kernel(),
            Self::ScanLineAccumulator(s) => s.cpu_kernel(),
            Self::TileToScanline(s) => s.cpu_kernel(),
            Self::TileBlockToTile(s) => s.cpu_kernel(),
        }
    }
}
