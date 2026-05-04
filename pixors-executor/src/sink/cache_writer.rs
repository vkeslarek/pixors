use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::Buffer;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};

static CW_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static CW_OUTPUTS: &[PortDecl] = &[];
static CW_PORTS: PortSpec = PortSpec { inputs: CW_INPUTS, outputs: CW_OUTPUTS };

/// Writes tiles to disk as raw RGBA8, organised by MIP level.
///
/// Directory layout:
/// ```text
/// {cache_dir}/
///   mip_0/
///     tile_0_0_0.raw
///     tile_0_0_1.raw
///     ...
///   mip_1/
///     tile_1_0_0.raw
///     ...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheWriter {
    pub cache_dir: PathBuf,
}

impl Stage for CacheWriter {
    fn kind(&self) -> &'static str {
        "cache_writer"
    }

    fn ports(&self) -> &'static PortSpec {
        &CW_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(CacheWriterRunner {
            cache_dir: self.cache_dir.clone(),
        }))
    }
}

pub struct CacheWriterRunner {
    cache_dir: PathBuf,
}

impl CpuKernel for CacheWriterRunner {
    fn process(&mut self, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        let tile = match item {
            Item::Tile(t) => t,
            other => return Err(Error::internal(format!(
                "CacheWriter expected Tile, got {:?}",
                other.kind(),
            ))),
        };

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
        let expected = (w * h * 4) as usize;
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
            "[pixors] cache_writer: wrote mip={mip} tile=({tx},{ty}) {}×{} to {}",
            w, h, path.display(),
        );
        Ok(())
    }
}
