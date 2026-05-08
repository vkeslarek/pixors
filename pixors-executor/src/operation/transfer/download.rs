use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::data::neighborhood::{Neighborhood, NeighborhoodData};
use crate::data::tile::Tile;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor, ProcessorContext, Stage,
};

use crate::debug_stopwatch;

static DN_PORT_DECL: &PortDeclaration = &PortDeclaration {
    name: "data",
    kind: DataKind::Tile,
};

static DN_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Variable(DN_PORT_DECL),
    outputs: PortGroup::Variable(DN_PORT_DECL),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download;

impl Stage for Download {
    fn kind(&self) -> &'static str {
        "download"
    }

    fn ports(&self) -> &'static PortSpecification {
        &DN_PORTS
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(DownloadProcessor::new()))
    }
}

pub struct DownloadProcessor;

impl DownloadProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for DownloadProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("download");
        match item {
            Item::Tile(tile) => {
                if tile.data.is_cpu() {
                    ctx.emit.emit(Item::Tile(tile));
                    return Ok(());
                }
                let gpu = ctx
                    .gpu
                    .as_ref()
                    .ok_or_else(|| Error::internal("GPU unavailable for download"))?;
                let scheduler = gpu.scheduler();
                scheduler.flush(); // ensure pending GPU dispatches complete before reading
                let gbuf = tile.data.as_gpu().unwrap();
                let bytes = scheduler.read_from_buffer(gbuf.buffer(), 0, gbuf.requested_size);
                ctx.emit.emit(Item::Tile(Tile::new(
                    tile.coord,
                    tile.meta,
                    Buffer::cpu(bytes),
                )));
                Ok(())
            }
            Item::Neighborhood(nbhd) => match nbhd.data {
                NeighborhoodData::Gpu {
                    consolidated,
                    tile_infos,
                } => {
                    let gpu = ctx
                        .gpu
                        .as_ref()
                        .ok_or_else(|| Error::internal("GPU unavailable for nbhd download"))?;
                    let scheduler = gpu.scheduler();
                    scheduler.flush();

                    let mut tiles = Vec::new();
                    for info in &tile_infos {
                        let bytes = scheduler.read_from_buffer(
                            consolidated.buffer(),
                            info.data_offset,
                            info.tile_size_bytes,
                        );
                        let tile = Tile::new(
                            crate::data::tile::TileCoord {
                                mip_level: nbhd.center.mip_level,
                                tx: info.px / nbhd.tile_size,
                                ty: info.py / nbhd.tile_size,
                                px: info.px,
                                py: info.py,
                                width: info.width,
                                height: info.height,
                                tile_size: nbhd.tile_size,
                                image_width: nbhd.image_width,
                                image_height: nbhd.image_height,
                            },
                            nbhd.meta,
                            Buffer::cpu(bytes),
                        );
                        tiles.push(tile);
                    }

                    let cpu_nbhd = Neighborhood::new_cpu(
                        nbhd.radius,
                        nbhd.center,
                        tiles,
                        nbhd.edge,
                        nbhd.meta,
                        nbhd.image_width,
                        nbhd.image_height,
                        nbhd.tile_size,
                    );
                    ctx.emit.emit(Item::Neighborhood(cpu_nbhd));
                    Ok(())
                }
                NeighborhoodData::Cpu { .. } => {
                    // Already CPU — pass through
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
