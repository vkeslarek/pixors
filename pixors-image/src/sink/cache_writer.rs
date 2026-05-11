use std::path::PathBuf;

use pixors_engine::data::buffer::Buffer;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{Consumer, DataKind, InPortSpecification, PortDeclaration, PortGroup};

static CW_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static CW_IN_PORTS: InPortSpecification = InPortSpecification {
    ports: PortGroup::Fixed(CW_INPUTS),
};

/// Writes tiles to disk as raw bytes, organised by MIP level.
#[derive(Debug, Clone)]
pub struct CacheWriter {
    pub cache_dir: PathBuf,
}

impl Consumer for CacheWriter {
    fn kind(&self) -> &'static str {
        "cache_writer"
    }

    fn in_ports(&self) -> &'static InPortSpecification {
        &CW_IN_PORTS
    }

    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = pixors_engine::stage::ProcessorContext::take_tile(item)?;
        let mip = tile.coord.mip_level;
        let tx = tile.coord.tx;
        let ty = tile.coord.ty;
        let w = tile.coord.width;
        let h = tile.coord.height;
        if w == 0 || h == 0 {
            return Ok(());
        }

        let data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => return Err(Error::internal("CacheWriter requires CPU tiles")),
        };
        let bpp = tile.meta.format.bytes_per_pixel();
        let expected = w as usize * h as usize * bpp;
        if data.len() != expected {
            return Err(Error::internal(format!(
                "CacheWriter tile size mismatch: expected {expected} bytes, got {}",
                data.len(),
            )));
        }

        let dir = self.cache_dir.join(format!("mip_{mip}"));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("tile_{mip}_{tx}_{ty}.raw"));
        std::fs::write(&path, data)?;
        tracing::debug!(
            "[pixors] cache_writer: wrote mip={mip} tile=({tx},{ty}) {w}×{h} to {}",
            path.display()
        );
        Ok(())
    }
}
