use crate::error::Error;
use crate::graph::item::Item;

/// Capacity of inter-runner channels. Bounded for backpressure.
pub const CHANNEL_BOUND: usize = 16;

/// `Some(item)` = data; `None` = end-of-stream sentinel.
pub type ItemSender = std::sync::mpsc::SyncSender<Option<Item>>;
pub type ItemReceiver = std::sync::mpsc::Receiver<Option<Item>>;

/// A Runner owns a thread of execution. The framework creates one Runner per
/// chain of stages on the same device. Runners communicate via bounded channels
/// — full channel → sender blocks, providing backpressure automatically.
pub trait Runner: Send {
    fn run(
        self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<ItemSender>,
    ) -> Result<(), Error>;
}
