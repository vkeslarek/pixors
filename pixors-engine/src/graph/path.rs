use std::sync::Arc;

use crate::stage::Stage;

/// A linear chain of stages — an ordered sequence that can be attached
/// to a `PathBuilder` or `ExecGraph`. Represents a partial pipeline segment.
#[derive(Debug, Clone, Default)]
pub struct Path {
    stages: Vec<Arc<dyn Stage + Send + Sync>>,
}

impl Path {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn push(mut self, stage: Arc<dyn Stage + Send + Sync>) -> Self {
        self.stages.push(stage);
        self
    }

    pub fn stages(&self) -> &[Arc<dyn Stage + Send + Sync>] {
        &self.stages
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}
