use crate::error::Error;
use crate::graph::item::Item;
use crate::graph::routed::Routed;

pub const CHANNEL_BOUND: usize = 16;

pub type RoutedItem = Routed<Item>;
pub type ItemSender = std::sync::mpsc::SyncSender<Option<RoutedItem>>;
pub type ItemReceiver = std::sync::mpsc::Receiver<Option<RoutedItem>>;

pub trait Runner: Send {
    fn run(
        self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<(ItemSender, u16)>,
    ) -> Result<(), Error>;
}
