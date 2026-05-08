use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::data::buffer::Buffer;
use crate::data::neighborhood::{Neighborhood, NeighborhoodData, TileGpuInfo};
use crate::data::tile::Tile;
use crate::error::Error;
use crate::gpu::pool::GpuBuffer;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor, ProcessorContext, Stage,
};

use crate::debug_stopwatch;

static UP_PORT_DECL: &PortDeclaration = &PortDeclaration {
    name: "data",
    kind: DataKind::Tile,
};

static UP_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Variable(UP_PORT_DECL),
    outputs: PortGroup::Variable(UP_PORT_DECL),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload;

impl Stage for Upload {
    fn kind(&self) -> &'static str {
        "upload"
    }

    fn ports(&self) -> &'static PortSpecification {
        &UP_PORTS
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(UploadProcessor::new()))
    }
}

pub struct UploadProcessor;

impl UploadProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for UploadProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("upload");
        match item {
            Item::Tile(tile) => {
                if tile.data.is_gpu() {
                    ctx.emit.emit(Item::Tile(tile));
                    return Ok(());
                }
                let gpu = ctx
                    .gpu
                    .as_ref()
                    .ok_or_else(|| Error::internal("GPU unavailable for upload"))?;
                let bytes: &[u8] = tile.data.as_cpu_slice().unwrap();
                let gbuf = gpu.scheduler().upload_bytes(bytes);
                ctx.emit.emit(Item::Tile(Tile::new(
                    tile.coord,
                    tile.meta,
                    Buffer::Gpu(Arc::new(gbuf)),
                )));
                Ok(())
            }
            Item::Neighborhood(nbhd) => match nbhd.data {
                NeighborhoodData::Cpu { tiles } => {
                    let gpu = ctx
                        .gpu
                        .as_ref()
                        .ok_or_else(|| Error::internal("GPU unavailable for neighborhood upload"))?;
                    let scheduler = gpu.scheduler();

                    tracing::debug!(
                        "[upload] Neighborhood Cpu→Gpu: {} tiles, center=({},{})",
                        tiles.len(),
                        nbhd.center.px,
                        nbhd.center.py,
                    );

                    // Upload each CPU tile to GPU, track metadata for consolidation
                    let mut gpu_tiles: Vec<(Arc<GpuBuffer>, &Tile)> = Vec::new();
                    let mut total_bytes = 0u64;
                    for tile in &tiles {
                        let data = match &tile.data {
                            Buffer::Cpu(v) => v.as_slice(),
                            Buffer::Gpu(g) => {
                                gpu_tiles.push((g.clone(), tile));
                                total_bytes += g.requested_size;
                                continue;
                            }
                        };
                        let gbuf = scheduler.upload_bytes(data);
                        total_bytes += gbuf.requested_size;
                        gpu_tiles.push((Arc::new(gbuf), tile));
                    }

                    // Consolidate into single buffer
                    let consolidated = Arc::new(scheduler.allocate_buffer(total_bytes));
                    let mut tile_infos = Vec::new();
                    let mut offset = 0u64;
                    for (gbuf, tile) in &gpu_tiles {
                        scheduler.copy_slice(
                            gbuf.buffer(),
                            0,
                            consolidated.buffer(),
                            offset,
                            gbuf.requested_size,
                        );
                        tile_infos.push(TileGpuInfo {
                            px: tile.coord.px,
                            py: tile.coord.py,
                            width: tile.coord.width,
                            height: tile.coord.height,
                            data_offset: offset,
                            tile_size_bytes: gbuf.requested_size,
                        });
                        offset += gbuf.requested_size;
                    }

                    let gpu_nbhd = Neighborhood::new_gpu(
                        nbhd.radius,
                        nbhd.center,
                        consolidated,
                        tile_infos,
                        nbhd.edge,
                        nbhd.meta,
                        nbhd.image_width,
                        nbhd.image_height,
                        nbhd.tile_size,
                    );
                    tracing::debug!(
                        "[upload] consolidated {} bytes, {} tile_infos",
                        total_bytes,
                        gpu_nbhd.data.tile_infos().len(),
                    );
                    ctx.emit.emit(Item::Neighborhood(gpu_nbhd));
                    Ok(())
                }
                NeighborhoodData::Gpu { .. } => {
                    // Already GPU — pass through
                    ctx.emit.emit(Item::Neighborhood(nbhd));
                    Ok(())
                }
            },
            other => {
                ctx.emit.emit(other);
                Ok(())
            }
        }
    }
}
