use std::sync::Arc;

use crate::common::pixel::meta::PixelMeta;
use crate::data::tile::Tile;
use crate::data::tile::TileCoord;
use crate::gpu::pool::GpuBuffer;

/// Metadata for a tile stored in a GPU consolidated buffer.
#[derive(Debug, Clone, Copy)]
pub struct TileGpuInfo {
    pub px: i32,
    pub py: i32,
    pub width: u32,
    pub height: u32,
    pub data_offset: u64,
    pub tile_size_bytes: u64,
}

/// Per-device storage for neighbourhood tile data.
#[derive(Debug, Clone)]
pub enum NeighborhoodData {
    /// CPU path: tiles stored as owned `Tile` structs (pointer accumulation).
    Cpu { tiles: Vec<Tile> },
    /// GPU path: all tiles concatenated into a single contiguous GPU buffer
    /// with per-tile metadata for coordinate lookup.
    Gpu {
        consolidated: Arc<GpuBuffer>,
        tile_infos: Vec<TileGpuInfo>,
    },
}

impl NeighborhoodData {
    pub fn is_cpu(&self) -> bool {
        matches!(self, Self::Cpu { .. })
    }

    pub fn is_gpu(&self) -> bool {
        matches!(self, Self::Gpu { .. })
    }

    /// Iterate over tiles (CPU path only). Returns empty for GPU data.
    pub fn tiles_cpu(&self) -> &[Tile] {
        match self {
            Self::Cpu { tiles } => tiles,
            Self::Gpu { .. } => &[],
        }
    }

    /// Iterate over tile GPU metadata (GPU path only). Returns empty for CPU data.
    pub fn tile_infos(&self) -> &[TileGpuInfo] {
        match self {
            Self::Cpu { .. } => &[],
            Self::Gpu { tile_infos, .. } => tile_infos,
        }
    }
}

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
    pub data: NeighborhoodData,
    pub edge: EdgeCondition,
    pub meta: PixelMeta,
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
}

impl Neighborhood {
    // ── Cpu constructor (backwards-compatible) ──────────────────────────────

    pub fn new_cpu(
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
            data: NeighborhoodData::Cpu { tiles },
            edge,
            meta,
            image_width,
            image_height,
            tile_size,
        }
    }

    // ── Gpu constructor ─────────────────────────────────────────────────────

    pub fn new_gpu(
        radius: u32,
        center: TileCoord,
        consolidated: Arc<GpuBuffer>,
        tile_infos: Vec<TileGpuInfo>,
        edge: EdgeCondition,
        meta: PixelMeta,
        image_width: u32,
        image_height: u32,
        tile_size: u32,
    ) -> Self {
        Self {
            radius,
            center,
            data: NeighborhoodData::Gpu {
                consolidated,
                tile_infos,
            },
            edge,
            meta,
            image_width,
            image_height,
            tile_size,
        }
    }

    pub fn tile_at(&self, tx: u32, ty: u32) -> Option<&Tile> {
        match &self.data {
            NeighborhoodData::Cpu { tiles } => {
                tiles.iter().find(|t| t.coord.tx == tx && t.coord.ty == ty)
            }
            NeighborhoodData::Gpu { .. } => None,
        }
    }
}
