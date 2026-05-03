use crate::egraph::emitter::Emitter;
use crate::egraph::item::Item;
use crate::egraph::runner::OperationRunner;
use crate::error::Error;

pub struct ColorConvertRunner;

impl OperationRunner for ColorConvertRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        match item {
            Item::Tile(t) => {
                emit.emit(Item::Tile(t));
                Ok(())
            }
            other => Err(Error::internal(format!(
                "expected Tile, got ScanLine/Neighborhood"
            ))),
        }
    }
}
