use crate::error::Error;
use crate::pipeline::emitter::Emitter;

pub mod blur;
pub mod color;
pub mod mip;
pub mod neighborhood;

pub trait Operation: Send + 'static {
    type In: Send + 'static;
    type Out: Send + 'static;

    fn name(&self) -> &'static str;
    fn cost(&self) -> f32 { 1.0 }

    fn process(&mut self, item: Self::In, emit: &mut Emitter<Self::Out>) -> Result<(), Error>;

    fn finish(&mut self, _emit: &mut Emitter<Self::Out>) -> Result<(), Error> { Ok(()) }
}
