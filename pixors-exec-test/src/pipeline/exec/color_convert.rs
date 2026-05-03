use serde::{Deserialize, Serialize};

use crate::pipeline::exec_graph::emitter::Emitter;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::OperationRunner;
use super::{Device, Stage};
use crate::error::Error;
use crate::debug_stopwatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConvert {
    pub target: String,
}

impl Stage for ColorConvert {
    fn kind(&self) -> &'static str { "color_convert" }
    fn device(&self) -> Device { Device::Cpu }
    fn allocates_output(&self) -> bool { false }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(ColorConvertRunner))
    }
}

pub struct ColorConvertRunner;

impl OperationRunner for ColorConvertRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
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
