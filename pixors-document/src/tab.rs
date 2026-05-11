use std::path::Path;

use crate::document::{Document, LayerNode, NodeId};
use crate::session::SessionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

#[derive(Debug)]
pub struct Tab {
    pub id: TabId,
    pub document: Document,
    pub session: SessionState,
}

#[derive(Debug, Clone, Default)]
pub struct TabView {
    pub active_mip: u32,
    pub loading: bool,
    pub progress: f32,
}

impl Tab {
    pub fn title(&self) -> &str { "untitled" } // desktop derives from DocumentView
    pub fn desc_width(&self) -> u32 { self.document.canvas.width }
    pub fn desc_height(&self) -> u32 { self.document.canvas.height }
    pub fn dpi(&self) -> Option<pixors_image::image::Dpi> { None } // TODO: store in AssetStore
    pub fn desc_color_space(&self) -> pixors_engine::common::color::space::ColorSpace {
        self.document.canvas.working_color_space
    }
    pub fn source_path(&self) -> Option<&Path> { self.document.assets.primary_path.as_deref() }
}
