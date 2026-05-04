use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::data::Buffer;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints,
};

pub type CacheCommitFn = Box<
    dyn Fn(
            u32,  // mip_level
            u32,  // tx
            u32,  // ty
            u32,  // px
            u32,  // py
            u32,  // width
            u32,  // height
            &[u8], // RGBA8 bytes
        ) + Send
        + Sync,
>;

static CACHE_SINK: OnceLock<Arc<CacheCommitFn>> = OnceLock::new();

pub fn install_viewport_cache_sink(f: CacheCommitFn) {
    let _ = CACHE_SINK.set(Arc::new(f));
}

pub fn viewport_cache_sink() -> Option<Arc<CacheCommitFn>> {
    CACHE_SINK.get().cloned()
}

static VCS_INPUTS: &[PortDecl] = &[PortDecl {
    name: "tile",
    kind: DataKind::Tile,
}];
static VCS_OUTPUTS: &[PortDecl] = &[];
static VCS_PORTS: PortSpec = PortSpec {
    inputs: VCS_INPUTS,
    outputs: VCS_OUTPUTS,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportCacheSink;

impl Stage for ViewportCacheSink {
    fn kind(&self) -> &'static str {
        "viewport_cache_sink"
    }

    fn ports(&self) -> &'static PortSpec {
        &VCS_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        let cb = CACHE_SINK.get().cloned()?;
        Some(Box::new(ViewportCacheSinkRunner { cb }))
    }
}

pub struct ViewportCacheSinkRunner {
    cb: Arc<CacheCommitFn>,
}

impl CpuKernel for ViewportCacheSinkRunner {
    fn process(&mut self, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        match item {
            Item::Tile(tile) => {
                let data = match &tile.data {
                    Buffer::Cpu(v) => v.as_slice(),
                    Buffer::Gpu(_) => {
                        return Err(Error::internal(
                            "ViewportCacheSink requires CPU tiles",
                        ))
                    }
                };
                (self.cb)(
                    tile.coord.mip_level,
                    tile.coord.tx,
                    tile.coord.ty,
                    tile.coord.px,
                    tile.coord.py,
                    tile.coord.width,
                    tile.coord.height,
                    data,
                );
                Ok(())
            }
            _ => Err(Error::internal("ViewportCacheSink expected Tile")),
        }
    }
}
