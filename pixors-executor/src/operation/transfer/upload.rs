use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::data::tile::Tile;
use crate::error::Error;
use crate::graph::item::Item;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage,
};

use crate::debug_stopwatch;

static UP_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static UP_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static UP_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(UP_INPUTS),
    outputs: PortGroup::Fixed(UP_OUTPUTS),
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload;

impl Stage for Upload {
    fn kind(&self) -> &'static str {
        "upload"
    }

    fn ports(&self) -> &'static PortSpecification {
        &UP_PORTS
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(UploadProcessor::new()))
    }
}

pub struct UploadProcessor;

impl UploadProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Processor for UploadProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("upload");
        let tile = ProcessorContext::take_tile(item)?;
        if tile.data.is_gpu() {
            ctx.emit.emit(Item::Tile(tile));
            return Ok(());
        }
        let gpu = ctx.gpu.as_ref().ok_or_else(|| Error::internal("GPU unavailable for upload"))?;
        let bytes: &[u8] = tile.data.as_cpu_slice().unwrap();
        let gbuf = gpu.scheduler().upload_bytes(bytes);
        ctx.emit.emit(Item::Tile(Tile::new(
            tile.coord,
            tile.meta,
            Buffer::Gpu(Arc::new(gbuf)),
        )));
        Ok(())
    }
}
