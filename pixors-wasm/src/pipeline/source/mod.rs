use crate::pipeline::emitter::Emitter;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Produces items for a pipeline. The framework spawns the thread and calls
/// [`run`](Source::run). Push items via `emit.emit()`.
pub trait Source: Send + 'static {
    type Item: Send + 'static;

    fn run(self, emit: &mut Emitter<Self::Item>, cancel: Arc<AtomicBool>);

    fn total(&self) -> Option<u32> {
        None
    }
}
