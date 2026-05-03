use crate::pipeline::egraph::emitter::Emitter;
use crate::pipeline::egraph::item::Item;
use crate::error::Error;

pub trait SourceRunner {
    fn run(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error>;
    fn finish(&mut self, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        Ok(())
    }
}

pub trait OperationRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error>;
    fn finish(&mut self, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        Ok(())
    }
}

pub trait SinkRunner {
    fn consume(&mut self, item: Item) -> Result<(), Error>;
    fn finish(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
