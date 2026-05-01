use crate::error::Error;

/// Consumes items one at a time. The framework handles channels and threads.
pub trait Sink: Send + Sync + 'static {
    type Item: Send + 'static;

    fn consume(&self, item: Self::Item) -> Result<(), Error>;

    fn finish(&self) {}
}
