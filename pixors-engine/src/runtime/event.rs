#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Progress { tag: u64, done: usize, total: usize },
    Done { tag: u64 },
    Error { tag: u64, message: String },
    Cancelled { tag: u64 },
}
