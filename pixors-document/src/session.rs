use std::path::{Path, PathBuf};

use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::PixelFormat;

use crate::document::Document;
use crate::document::NodeId;
use crate::history::History;
use crate::tab::SessionId;

#[derive(Debug, Clone, Default)]
pub struct ViewState {
    pub active_mip: u32,
    pub loading: bool,
    pub progress: f32,
}

/// Transient editing state for one session. Never serialized to disk.
#[derive(Debug)]
pub struct Transient {
    pub cache_dir: PathBuf,
    pub redraw_seq: u64,
    pub view: ViewState,
    pub active_node: Option<NodeId>,
    pub pipeline_running: bool,
    pub progress: f32,
}

impl Transient {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            redraw_seq: 0,
            view: ViewState::default(),
            active_node: None,
            pipeline_running: false,
            progress: 0.0,
        }
    }
}

/// A single editing session — the unit of work.
/// Owns a document, its undo history, and transient runtime state.
/// Desktop wraps this in a Tab; MCP uses it headlessly.
#[derive(Debug)]
pub struct Session {
    pub id: SessionId,
    pub document: Document,
    pub history: History,
    pub transient: Transient,
    pub working_format: PixelFormat,
    pub working_color_space: ColorSpace,
    pub display_format: PixelFormat,
    pub display_color_space: ColorSpace,
}

impl Session {
    pub fn layer_cache_dir(&self, node_id: NodeId) -> PathBuf {
        crate::document::cache::layer_cache_dir(&self.transient.cache_dir, node_id)
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.document.assets.primary_path.as_deref()
    }
}
