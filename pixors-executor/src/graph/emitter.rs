use crate::graph::routed::Routed;

pub struct Emitter<T> {
    items: Vec<Routed<T>>,
}

impl<T> Emitter<T> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }
    pub fn emit_to(&mut self, port: u16, item: T) {
        self.items.push(Routed { port, payload: item });
    }
    pub fn emit(&mut self, item: T) {
        self.emit_to(0, item);
    }
    pub fn into_items(self) -> Vec<Routed<T>> {
        self.items
    }
}
