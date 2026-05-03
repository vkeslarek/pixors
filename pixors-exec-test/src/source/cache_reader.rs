use serde::{Deserialize, Serialize};

use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};
use crate::error::Error;

static CR_INPUTS: &[PortDecl] = &[];
static CR_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static CR_PORTS: PortSpec = PortSpec { inputs: CR_INPUTS, outputs: CR_OUTPUTS };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheReader {
    pub cache_id: String,
}

impl Stage for CacheReader {
    fn kind(&self) -> &'static str {
        "cache_reader"
    }

    fn ports(&self) -> &'static PortSpec {
        &CR_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadOnly,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        None
    }
}

pub struct CacheReaderRunner;

impl CpuKernel for CacheReaderRunner {
    fn process(&mut self, _item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        Err(Error::internal("cache_reader not implemented"))
    }
}
