#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Progress { done: usize, total: usize },
    Done,
    Error(String),
}
