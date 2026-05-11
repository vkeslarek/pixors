use crate::error::Error;

use super::context::ProcessorContext;
use super::kinds::{InOutPortSpecification, InPortSpecification, OutPortSpecification};
use super::node::StageHints;

// ── Producer ──────────────────────────────────────────────────────────────────

pub trait Producer: Send + Sync + std::fmt::Debug {
    fn kind(&self) -> &'static str;
    fn out_ports(&self) -> &'static OutPortSpecification;
    fn hints(&self) -> StageHints {
        StageHints::cpu()
    }
    fn source_items(&self) -> usize {
        0
    }
    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error>;
}

// ── Processor ─────────────────────────────────────────────────────────────────

pub trait Processor: Send + Sync + std::fmt::Debug {
    fn kind(&self) -> &'static str;
    fn in_out_ports(&self) -> &'static InOutPortSpecification;
    fn hints(&self) -> StageHints {
        StageHints::cpu()
    }
    fn work_multiplier(&self) -> f64 {
        1.0
    }
    fn process(
        &mut self,
        ctx: ProcessorContext<'_>,
        item: crate::graph::item::Item,
    ) -> Result<(), Error>;
    fn finish(&mut self, _ctx: ProcessorContext<'_>) -> Result<(), Error> {
        Ok(())
    }
}

// ── Consumer ──────────────────────────────────────────────────────────────────

pub trait Consumer: Send + Sync + std::fmt::Debug {
    fn kind(&self) -> &'static str;
    fn in_ports(&self) -> &'static InPortSpecification;
    fn hints(&self) -> StageHints {
        StageHints::cpu()
    }
    fn consume(&mut self, item: crate::graph::item::Item) -> Result<(), Error>;
    fn finish(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
