use std::collections::HashMap;
use std::path::PathBuf;

use crate::document::NodeId;

/// Live preview of an uncommitted adjustment parameter change.
/// Lives in SessionState — never goes to disk.
#[derive(Debug, Clone)]
pub struct PreviewState {
    /// Which node (adjustment) is being previewed.
    pub target_node: NodeId,
    /// Param name → override value. e.g. "radius" → F32(5.0)
    pub overrides: HashMap<String, AdjustmentValue>,
    /// Higher mip level for faster preview rendering.
    pub preview_mip: u32,
}

#[derive(Debug, Clone)]
pub enum AdjustmentValue {
    F32(f32),
    U32(u32),
    Bool(bool),
}

/// Transient session state. Never serialized to disk.
/// One per Tab. Contains viewport, selection, preview, pipeline status.
#[derive(Debug)]
pub struct SessionState {
    /// Root directory for disk-based tile cache. Machine-specific.
    pub cache_dir: PathBuf,
    /// Incremented on every document mutation. Triggers viewport redraw.
    pub redraw_seq: u64,
    /// Viewport camera, mip level, loading/progress indicators.
    pub view: crate::tab::TabView,
    /// Currently selected node in the UI (layer or adjustment).
    pub active_node: Option<NodeId>,
    /// Live preview override for uncommitted slider drag.
    pub active_preview: Option<PreviewState>,
    /// Position in the history mutation log (undo/redo cursor).
    pub history_cursor: usize,
    /// Whether a pipeline is currently running (modal lock for Apply mode).
    pub pipeline_running: bool,
    /// Loading progress 0.0..=1.0.
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
            history_cursor: 0,
            pipeline_running: false,
            progress: 0.0,
        }
    }
}
