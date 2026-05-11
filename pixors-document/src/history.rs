use std::fmt;
use std::sync::Arc;

use crate::document::Document;
use crate::mutation::DocumentMutation;

/// Mutation-based undo/redo history.
#[derive(Default)]
pub struct History {
    mutations: Vec<Arc<dyn DocumentMutation>>,
    cursor: usize,
}

impl fmt::Debug for History {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("History")
            .field("mutations", &self.mutations.len())
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl History {
    pub fn new() -> Self {
        Self { mutations: Vec::new(), cursor: 0 }
    }

    pub fn push(&mut self, mutation: Arc<dyn DocumentMutation>, doc: &mut Document) {
        self.mutations.truncate(self.cursor);
        mutation.apply(doc);
        self.mutations.push(mutation);
        self.cursor = self.mutations.len();
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        if self.cursor == 0 { return None; }
        self.cursor -= 1;
        let label = self.mutations[self.cursor].label().to_string();
        self.mutations[self.cursor].undo(doc);
        Some(label)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        if self.cursor == self.mutations.len() { return None; }
        let label = self.mutations[self.cursor].label().to_string();
        self.mutations[self.cursor].apply(doc);
        self.cursor += 1;
        Some(label)
    }

    pub fn can_undo(&self) -> bool { self.cursor > 0 }
    pub fn can_redo(&self) -> bool { self.cursor < self.mutations.len() }

    pub fn past_labels(&self) -> impl Iterator<Item = &str> {
        self.mutations[..self.cursor].iter().map(|m| m.label())
    }
}
