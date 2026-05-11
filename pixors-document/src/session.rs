use std::collections::HashMap;
use std::path::PathBuf;

use crate::document::NodeId;
use crate::view::params::ParamValue;

/// Live preview of an uncommitted filter parameter change.
/// Lives in SessionState — never goes to disk.
#[derive(Debug, Clone)]
pub struct PreviewState {
    /// Which layer the previewed filter belongs to.
    pub layer_id: NodeId,
    /// Index within layer.filters.
    pub filter_index: usize,
    /// Param name → override value. e.g. "radius" → F32(5.0)
    pub overrides: HashMap<String, ParamValue>,
    /// Higher mip level for faster preview rendering.
    pub preview_mip: u32,
}

/// Transient session state. Never serialized to disk.
/// One per Tab. Contains viewport, selection, preview, pipeline status.
#[derive(Debug)]
pub struct SessionState {
    pub cache_dir: PathBuf,
    pub redraw_seq: u64,
    pub view: crate::tab::TabView,
    pub active_node: Option<NodeId>,
    pub active_preview: Option<PreviewState>,
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
            active_preview: None,
            pipeline_running: false,
            progress: 0.0,
        }
    }
}
