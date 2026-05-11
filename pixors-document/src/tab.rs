use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::history::History;
use crate::session::SessionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

#[derive(Debug)]
pub struct Tab {
    pub id: TabId,
    pub document: Document,
    pub history: History,
    pub session: SessionState,
}

#[derive(Debug, Clone, Default)]
pub struct TabView {
    pub active_mip: u32,
    pub loading: bool,
    pub progress: f32,
}

impl Tab {
    /// Path where tiles for a specific layer are stored on disk.
    pub fn layer_cache_dir(&self, node_id: crate::document::NodeId) -> std::path::PathBuf {
        self.session
            .cache_dir
            .join(format!("layer_{:016x}", node_id.0))
    }

    pub fn title(&self) -> &str {
        "untitled"
    }
    pub fn desc_width(&self) -> u32 {
        self.document.canvas.width
    }
    pub fn desc_height(&self) -> u32 {
        self.document.canvas.height
    }
    pub fn dpi(&self) -> Option<pixors_image::image::Dpi> {
        None
    }
    pub fn desc_color_space(&self) -> pixors_engine::common::color::space::ColorSpace {
        self.document.canvas.working_color_space
    }
    pub fn source_path(&self) -> Option<&Path> {
        self.document.assets.primary_path.as_deref()
    }
}
