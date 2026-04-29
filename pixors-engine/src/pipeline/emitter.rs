use std::sync::mpsc;

pub struct Emitter<Out: Send + 'static> {
    tx: mpsc::SyncSender<Out>,
}

impl<Out: Send + 'static> Emitter<Out> {
    pub fn new(tx: mpsc::SyncSender<Out>) -> Self { Self { tx } }
    pub fn emit(&mut self, item: Out) { let _ = self.tx.send(item); }
}
