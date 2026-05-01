use std::any::TypeId;

use crate::error::Error;
use crate::pipeline::runner::{RunnerOptions, RunnerKind};

pub mod file;

pub trait Source: Clone {
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

    fn run_cpu(&mut self, _emit: &mut crate::pipeline::runner::Emitter<Self::Output>) -> Result<(), Error> {
        Err(Error::internal("CPU runner not available"))
    }

    fn finish_cpu(&mut self, _emit: &mut crate::pipeline::runner::Emitter<Self::Output>) -> Result<(), Error> {
        Ok(())
    }
}

pub trait AnySource {
    fn output_type_id(&self) -> TypeId;
    fn output_type_name(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn params(&self) -> serde_json::Value;
    fn clone_source(&self) -> Box<dyn AnySource>;
    fn available_runners(&self) -> RunnerOptions;
    fn run_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any + Send>)) -> Result<(), Error>;
    fn finish_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any + Send>)) -> Result<(), Error>;
}

impl<S: Source + serde::Serialize + 'static> AnySource for S {
    fn output_type_id(&self) -> TypeId {
        TypeId::of::<S::Output>()
    }

    fn output_type_name(&self) -> &'static str {
        std::any::type_name::<S::Output>()
    }

    fn name(&self) -> &'static str {
        S::name(self)
    }

    fn params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn clone_source(&self) -> Box<dyn AnySource> {
        Box::new(self.clone())
    }

    fn available_runners(&self) -> RunnerOptions {
        S::available_runners(self)
    }

    fn run_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any + Send>)) -> Result<(), Error> {
        let mut emitter = crate::pipeline::runner::Emitter::new();
        S::run_cpu(self, &mut emitter)?;
        for item in emitter.into_items() {
            emit(Box::new(item));
        }
        Ok(())
    }

    fn finish_cpu_erased(&mut self, emit: &mut dyn FnMut(Box<dyn Any + Send>)) -> Result<(), Error> {
        let mut emitter = crate::pipeline::runner::Emitter::new();
        S::finish_cpu(self, &mut emitter)?;
        for item in emitter.into_items() {
            emit(Box::new(item));
        }
        Ok(())
    }
}
