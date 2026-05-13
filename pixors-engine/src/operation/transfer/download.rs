use crate::data::buffer::Buffer;
use crate::data::neighborhood::{Neighborhood, NeighborhoodData};
use crate::data::tile::Tile;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
};

use crate::debug_stopwatch;

static DN_PORT_DECL: &PortDeclaration = &PortDeclaration {
    name: "data",
    kind: DataKind::Tile,
};

static DN_PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Variable(DN_PORT_DECL),
    outputs: PortGroup::Variable(DN_PORT_DECL),
};

#[derive(Debug, Clone)]
pub struct Download;

impl Download {
    pub fn stage() -> crate::stage::Stage {
        crate::stage::Stage::Processor(Box::new(Self))
    }
}

impl Processor for Download {
    fn kind(&self) -> &'static str {
        "download"
    }

    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &DN_PORTS
    }

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
                scheduler.flush();
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

                    let mip_step = nbhd.tile_size.saturating_mul(1u32 << nbhd.center.mip_level);
                    let tiles_x = nbhd.image_width.div_ceil(mip_step) as i32;
                    let tiles_y = nbhd.image_height.div_ceil(mip_step) as i32;

                    let regions: Vec<_> = tile_infos
                        .iter()
                        .map(|info| (info.data_offset, info.tile_size_bytes))
                        .collect();
                    let batch_bytes =
                        scheduler.read_batch_from_buffer(consolidated.buffer(), &regions);

                    let mut tiles = Vec::new();
                    for (info, bytes) in tile_infos.iter().zip(batch_bytes.into_iter()) {
                        let logical_tx = info.px / nbhd.tile_size as i32;
                        let logical_ty = info.py / nbhd.tile_size as i32;
                        let gx = logical_tx.clamp(0, tiles_x - 1).max(0) as u32;
                        let gy = logical_ty.clamp(0, tiles_y - 1).max(0) as u32;

                        let tile = Tile::new(
                            crate::data::tile::TileCoord {
                                mip_level: nbhd.center.mip_level,
                                tx: gx,
                                ty: gy,
                                px: gx * nbhd.tile_size,
                                py: gy * nbhd.tile_size,
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
