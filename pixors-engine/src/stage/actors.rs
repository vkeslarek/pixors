use crate::error::Error;

use super::context::ProcessorContext;

// ── Producer ───────────────────────────────────────────────────────────────────

/// Active producer — emits items without receiving any input.
/// Used by source stages (image readers, cache loaders) that kickstart a pipeline.
pub trait Producer: Send {
    fn produce(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error>;
    /// How many items this source emits total (for progress estimation).
    fn source_items(&self) -> usize {
        0
    }
}

// ── Processor ──────────────────────────────────────────────────────────────────

pub trait Processor: Send {
    fn process(
        &mut self,
        ctx: ProcessorContext<'_>,
        item: crate::graph::item::Item,
    ) -> Result<(), Error>;
    fn finish(&mut self, _ctx: ProcessorContext<'_>) -> Result<(), Error> {
        Ok(())
    }
}

// ── Consumer ───────────────────────────────────────────────────────────────────

/// Terminal consumer — receives items but never emits.
/// Used by sink stages (cache writers, tile sinks, viewport cache).
pub trait Consumer: Send {
    fn consume(&mut self, item: crate::graph::item::Item) -> Result<(), Error>;
    fn finish(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
