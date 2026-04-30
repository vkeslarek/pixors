use std::sync::{mpsc, Arc};

pub struct Emitter<Out: Send + 'static> {
    tx: mpsc::SyncSender<Out>,
}

impl<Out: Send + 'static> Emitter<Out> {
    pub fn new(tx: mpsc::SyncSender<Out>) -> Self { Self { tx } }
    pub fn emit(&mut self, item: Out) { let _ = self.tx.send(item); }
    pub fn emit_arc(&mut self, item: Arc<Out>) where Out: Clone { let _ = self.tx.send((*item).clone()); }
}
