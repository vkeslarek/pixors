use std::any::{Any, TypeId};

use crate::error::Error;
use crate::pipeline::runner::{RunnerOptions, RunnerKind};

pub mod to_neighborhood;

pub mod to_tile;

pub trait Converter: Clone {
    type Input;
    type Output;

    fn name(&self) -> &'static str;

    fn available_runners(&self) -> RunnerOptions {
        RunnerOptions {
            cpu: false,
            gpu: false,
            preferred: RunnerKind::Cpu,
            modify_in_place: false,
        }
    }

    fn process_cpu(&mut self, _input: Self::Input, _emit: &mut crate::pipeline::runner::Emitter<Self::Output>) -> Result<(), Error> {
        Err(Error::internal("CPU runner not available"))
    }

    fn finish_cpu(&mut self, _emit: &mut crate::pipeline::runner::Emitter<Self::Output>) -> Result<(), Error> {
        Ok(())
    }
}

pub trait AnyConverter {
    fn input_type_id(&self) -> TypeId;
    fn input_type_name(&self) -> &'static str;
    fn output_type_id(&self) -> TypeId;
    fn output_type_name(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn params(&self) -> serde_json::Value;
    fn clone_converter(&self) -> Box<dyn AnyConverter>;
    fn available_runners(&self) -> RunnerOptions;
    fn process_cpu_erased(&mut self, input: Box<dyn Any>, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error>;
    fn finish_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error>;
}

impl<C: Converter + serde::Serialize + 'static> AnyConverter for C {
    fn input_type_id(&self) -> TypeId {
        TypeId::of::<C::Input>()
    }

    fn input_type_name(&self) -> &'static str {
        std::any::type_name::<C::Input>()
    }

    fn output_type_id(&self) -> TypeId {
        TypeId::of::<C::Output>()
    }

    fn output_type_name(&self) -> &'static str {
        std::any::type_name::<C::Output>()
    }

    fn name(&self) -> &'static str {
        C::name(self)
    }

    fn params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn clone_converter(&self) -> Box<dyn AnyConverter> {
        Box::new(self.clone())
    }

    fn available_runners(&self) -> RunnerOptions {
        C::available_runners(self)
    }

    fn process_cpu_erased(&mut self, input: Box<dyn Any>, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error> {
        let typed: C::Input = *input.downcast().map_err(|_| Error::internal("type mismatch in converter"))?;
        let mut emitter = crate::pipeline::runner::Emitter::new();
        C::process_cpu(self, typed, &mut emitter)?;
        for item in emitter.into_items() {
            emit(Box::new(item));
        }
        Ok(())
    }

    fn finish_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error> {
        let mut emitter = crate::pipeline::runner::Emitter::new();
        C::finish_cpu(self, &mut emitter)?;
        for item in emitter.into_items() {
            emit(Box::new(item));
        }
        Ok(())
    }
}
