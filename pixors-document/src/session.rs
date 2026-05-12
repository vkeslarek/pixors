use std::path::PathBuf;

use crate::document::NodeId;

/// Transient session state. Never serialized to disk.
/// One per Tab. Contains viewport, selection, pipeline status.
#[derive(Debug)]
pub struct SessionState {
    pub cache_dir: PathBuf,
    pub redraw_seq: u64,
    pub view: crate::tab::TabView,
    pub active_node: Option<NodeId>,
    pub pipeline_running: bool,
    pub progress: f32,
}

impl SessionState {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            redraw_seq: 0,
            view: crate::tab::TabView::default(),
            active_node: None,
            pipeline_running: false,
            progress: 0.0,
        }
    }
}
