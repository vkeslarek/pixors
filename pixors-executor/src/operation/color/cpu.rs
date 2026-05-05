use serde::{Deserialize, Serialize};

use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortGroup, PortSpec, Stage, StageHints};

use crate::error::Error;

use crate::debug_stopwatch;


static CC_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];

static CC_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];

static CC_PORTS: PortSpec = PortSpec { inputs: PortGroup::Fixed(CC_INPUTS), outputs: PortGroup::Fixed(CC_OUTPUTS) };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub target: String,
}

impl Stage for ColorConvert {
    fn kind(&self) -> &'static str { "color_convert" }

    fn ports(&self) -> &'static PortSpec {
        &CC_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(ColorConvertRunner))
    }
}

pub struct ColorConvertRunner;

impl CpuKernel for ColorConvertRunner {
    fn process(&mut self, _port: u16, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("color_convert");
        match item {
            Item::Tile(t) => {
                emit.emit(Item::Tile(t));
                Ok(())
            }
            _other => Err(Error::internal(
                "expected Tile, got ScanLine/Neighborhood".to_string()
            )),
        }
    }
}
