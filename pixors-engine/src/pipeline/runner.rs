pub struct Emitter<T> {
    items: Vec<T>,
}

impl<T> Emitter<T> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn emit(&mut self, item: T) {
        self.items.push(item);
    }

    pub fn into_items(self) -> Vec<T> {
        self.items
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerKind {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone)]
pub struct RunnerOptions {
    pub cpu: bool,
    pub gpu: bool,
    pub preferred: RunnerKind,
    pub modify_in_place: bool,
}
