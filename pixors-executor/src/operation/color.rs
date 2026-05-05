use serde::{Deserialize, Serialize};

use crate::graph::item::Item;
use crate::stage::{BufferAccess, Processor, ProcessorContext, DataKind, PortDeclaration, PortGroup, PortSpecification, Stage, StageHints};

use crate::error::Error;

use crate::debug_stopwatch;


static CC_INPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static CC_OUTPUTS: &[PortDeclaration] = &[PortDeclaration { name: "tile", kind: DataKind::Tile }];

static CC_PORTS: PortSpecification = PortSpecification { inputs: PortGroup::Fixed(CC_INPUTS), outputs: PortGroup::Fixed(CC_OUTPUTS) };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub target: String,
}

impl Stage for ColorConvert {
    fn kind(&self) -> &'static str { "color_convert" }

    fn ports(&self) -> &'static PortSpecification {
        &CC_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(ColorConvertProcessor))
    }
}

pub struct ColorConvertProcessor;

impl Processor for ColorConvertProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("color_convert");
        let t = ProcessorContext::take_tile(item)?;
        ctx.emit.emit(Item::Tile(t));
        Ok(())
    }
}
