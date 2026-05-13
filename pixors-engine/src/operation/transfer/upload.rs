use std::sync::Arc;

use crate::data::buffer::Buffer;
use crate::data::neighborhood::{Neighborhood, NeighborhoodData, TileGpuInfo};
use crate::data::tile::Tile;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
};

use crate::debug_stopwatch;

static UP_PORT_DECL: &PortDeclaration = &PortDeclaration {
    name: "data",
    kind: DataKind::Tile,
};

static UP_PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Variable(UP_PORT_DECL),
    outputs: PortGroup::Variable(UP_PORT_DECL),
};

#[derive(Debug, Clone)]
pub struct Upload;

impl Upload {
    pub fn stage() -> crate::stage::Stage {
        crate::stage::Stage::Processor(Box::new(Self))
    }
}

impl Processor for Upload {
    fn kind(&self) -> &'static str {
        "upload"
    }

    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &UP_PORTS
    }

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
                    let gpu = ctx.gpu.as_ref().ok_or_else(|| {
                        Error::internal("GPU unavailable for neighborhood upload")
                    })?;
                    let scheduler = gpu.scheduler();

                    tracing::debug!(
                        "[upload] Neighborhood Cpu→Gpu: {} tiles, center=({},{})",
                        tiles.len(),
                        nbhd.center.px,
                        nbhd.center.py,
                    );

                    let mut total_bytes = 0u64;
                    for tile in &tiles {
                        total_bytes += match &tile.data {
                            Buffer::Cpu(v) => v.len() as u64,
                            Buffer::Gpu(g) => g.requested_size,
                        };
                    }

                    let consolidated = Arc::new(scheduler.allocate_buffer(total_bytes));
                    let mut tile_infos = Vec::new();
                    let mut offset = 0u64;
                    for tile in &tiles {
                        let size_bytes = match &tile.data {
                            Buffer::Cpu(v) => {
                                let data = v.as_slice();
                                let len = data.len() as u64;
                                let aligned_len = (len + 3) & !3;
                                if aligned_len as usize == data.len() {
                                    gpu.queue()
                                        .write_buffer(consolidated.buffer(), offset, data);
                                } else {
                                    let mut padded = data.to_vec();
                                    padded.resize(aligned_len as usize, 0);
                                    gpu.queue().write_buffer(
                                        consolidated.buffer(),
                                        offset,
                                        &padded,
                                    );
                                }
                                len
                            }
                            Buffer::Gpu(g) => {
                                scheduler.copy_slice(
                                    g.buffer(),
                                    0,
                                    consolidated.buffer(),
                                    offset,
                                    g.requested_size,
                                );
                                g.requested_size
                            }
                        };
                        tile_infos.push(TileGpuInfo {
                            px: tile.coord.px as i32,
                            py: tile.coord.py as i32,
                            width: tile.coord.width,
                            height: tile.coord.height,
                            data_offset: offset,
                            tile_size_bytes: size_bytes,
                        });
                        offset += size_bytes;
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
