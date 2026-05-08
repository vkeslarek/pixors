use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{Consumer, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage};

static CW_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static CW_OUTPUTS: &[PortDeclaration] = &[];

static CW_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(CW_INPUTS),
    outputs: PortGroup::Fixed(CW_OUTPUTS),
};

/// Writes tiles to disk as raw RGBA8, organised by MIP level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheWriter {
    pub cache_dir: PathBuf,
}

impl Stage for CacheWriter {
    fn kind(&self) -> &'static str {
        "cache_writer"
    }

    fn ports(&self) -> &'static PortSpecification {
        &CW_PORTS
    }

    fn consumer(&self) -> Option<Box<dyn Consumer>> {
        Some(Box::new(CacheWriterConsumer {
            cache_dir: self.cache_dir.clone(),
            use_compression: false,
        }))
    }
}

pub struct CacheWriterConsumer {
    cache_dir: PathBuf,
    use_compression: bool,
}

impl Consumer for CacheWriterConsumer {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let tile = crate::stage::ProcessorContext::take_tile(item)?;

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
            Buffer::Gpu(_) => {
                return Err(Error::internal("CacheWriter requires CPU tiles"));
            }
        };
        let bpp = tile.meta.format.bytes_per_pixel() as usize;
        let expected = (w as usize * h as usize * bpp) as usize;
        if data.len() != expected {
            return Err(Error::internal(format!(
                "CacheWriter tile size mismatch: expected {expected} bytes, got {}",
                data.len(),
            )));
        }

        let dir = self.cache_dir.join(format!("mip_{mip}"));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("tile_{mip}_{tx}_{ty}.raw"));

        let data_to_write = if self.use_compression {
            lz4_flex::compress_prepend_size(data)
        } else {
            data.to_vec()
        };

        std::fs::write(&path, &data_to_write)?;
        tracing::debug!(
            "[pixors] cache_writer: wrote mip={mip} tile=({tx},{ty}) {}×{} to {}",
            w,
            h,
            path.display()
        );
        Ok(())
    }
}
