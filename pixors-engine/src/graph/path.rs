use crate::stage::Stage;

#[derive(Debug, Default)]
pub struct Path {
    stages: Vec<Stage>,
}

impl Path {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }
    pub fn push(mut self, stage: Stage) -> Self {
        self.stages.push(stage);
        self
    }
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}
