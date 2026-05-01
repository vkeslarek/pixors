use std::any::TypeId;

use crate::error::Error;
use crate::pipeline::runner::{RunnerOptions, RunnerKind};

pub mod file;

pub trait Sink: Clone {
    type Input;

    fn name(&self) -> &'static str;

    fn available_runners(&self) -> RunnerOptions {
        RunnerOptions {
            cpu: false,
            gpu: false,
            preferred: RunnerKind::Cpu,
            modify_in_place: false,
        }
    }

    fn consume_cpu(&mut self, _input: Self::Input) -> Result<(), Error> {
        Err(Error::internal("CPU runner not available"))
    }

    fn finish_cpu(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub trait AnySink {
    fn input_type_id(&self) -> TypeId;
    fn input_type_name(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn params(&self) -> serde_json::Value;
    fn clone_sink(&self) -> Box<dyn AnySink>;
    fn available_runners(&self) -> RunnerOptions;
    fn consume_cpu_erased(&mut self, input: Box<dyn Any + Send>) -> Result<(), Error>;
    fn finish_cpu_erased(&mut self) -> Result<(), Error>;
}

impl<S: Sink + serde::Serialize + 'static> AnySink for S {
    fn input_type_id(&self) -> TypeId {
        TypeId::of::<S::Input>()
    }

    fn input_type_name(&self) -> &'static str {
        std::any::type_name::<S::Input>()
    }

    fn name(&self) -> &'static str {
        S::name(self)
    }

    fn params(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn clone_sink(&self) -> Box<dyn AnySink> {
        Box::new(self.clone())
    }

    fn available_runners(&self) -> RunnerOptions {
        S::available_runners(self)
    }

    fn consume_cpu_erased(&mut self, input: Box<dyn Any + Send>) -> Result<(), Error> {
        let typed: S::Input = *input.downcast().map_err(|_| Error::internal("type mismatch in sink"))?;
        S::consume_cpu(self, typed)
    }

    fn finish_cpu_erased(&mut self) -> Result<(), Error> {
        S::finish_cpu(self)
    }
}
