use std::any::{Any, TypeId};

use crate::error::Error;
use crate::pipeline::runner::{RunnerOptions, RunnerKind};

pub mod blur;

pub trait Operation: Clone {
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

pub trait AnyOperation {
    fn input_type_id(&self) -> TypeId;
    fn input_type_name(&self) -> &'static str;
    fn output_type_id(&self) -> TypeId;
    fn output_type_name(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn params(&self) -> serde_json::Value;
    fn clone_operation(&self) -> Box<dyn AnyOperation>;
    fn available_runners(&self) -> RunnerOptions;
    fn process_cpu_erased(&mut self, input: Box<dyn Any>, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error>;
    fn finish_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error>;
}

impl<O: Operation + serde::Serialize + 'static> AnyOperation for O {
    fn input_type_id(&self) -> TypeId {
        TypeId::of::<O::Input>()
    }

    fn input_type_name(&self) -> &'static str {
        std::any::type_name::<O::Input>()
    }

    fn output_type_id(&self) -> TypeId {
        TypeId::of::<O::Output>()
    }

    fn output_type_name(&self) -> &'static str {
        std::any::type_name::<O::Output>()
    }

    fn name(&self) -> &'static str {
        O::name(self)
    }

    fn params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn clone_operation(&self) -> Box<dyn AnyOperation> {
        Box::new(self.clone())
    }

    fn available_runners(&self) -> RunnerOptions {
        O::available_runners(self)
    }

    fn process_cpu_erased(&mut self, input: Box<dyn Any>, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error> {
        let typed: O::Input = *input.downcast().map_err(|_| Error::internal("type mismatch in operation"))?;
        let mut emitter = crate::pipeline::runner::Emitter::new();
        O::process_cpu(self, typed, &mut emitter)?;
        for item in emitter.into_items() {
            emit(Box::new(item));
        }
        Ok(())
    }

    fn finish_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any>)) -> Result<(), Error> {
        let mut emitter = crate::pipeline::runner::Emitter::new();
        O::finish_cpu(self, &mut emitter)?;
        for item in emitter.into_items() {
            emit(Box::new(item));
        }
        Ok(())
    }
}
