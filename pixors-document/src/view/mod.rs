pub mod params;

use crate::document::canvas::CanvasInfo;
use crate::document::layer::{LayerFilter, LayerNode, PixelSource};
use crate::document::Document;
use crate::document::NodeId;
use crate::session::SessionState;

use params::ParamSpec;
use pixors_image::image::BlendMode;

// ── LayerPanelItem ──────────────────────────────────────────────────────

/// Flat, widget-ready representation of a layer.
/// Computed eagerly — no internal cache in phase 10.
#[derive(Debug, Clone)]
pub struct LayerPanelItem {
    pub id: NodeId,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub kind: LayerKind,
    pub depth: u8,          // always 0 in phase 10
    pub filter_count: usize,
    pub has_mask: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Pixel,
    SolidColor,
}

// ── DocumentView ────────────────────────────────────────────────────────

/// Derived, widget-ready view of a Document + SessionState.
/// No internal cache — computed eagerly.
pub struct DocumentView<'a> {
    document: &'a Document,
    session: &'a SessionState,
}

impl<'a> DocumentView<'a> {
    pub fn new(document: &'a Document, session: &'a SessionState) -> Self {
        Self { document, session }
    }

    pub fn layers_panel(&self) -> Vec<LayerPanelItem> {
        self.document.layers.iter().map(|l| LayerPanelItem {
            id: l.id,
            name: l.name.clone(),
            visible: l.visible,
            opacity: l.blend.opacity,
            blend_mode: l.blend.mode,
            kind: match &l.source {
                PixelSource::PrimaryAsset { .. } => LayerKind::Pixel,
                PixelSource::SolidColor { .. } => LayerKind::SolidColor,
            },
            depth: 0,
            filter_count: l.filters.len(),
            has_mask: l.mask.is_some(),
        }).collect()
    }

    pub fn active_layer(&self) -> Option<&LayerNode> {
        self.session.active_node
            .and_then(|id| self.document.find_layer(id))
    }

    pub fn active_layer_filters(&self) -> Option<&[LayerFilter]> {
        self.active_layer().map(|l| l.filters.as_slice())
    }

    pub fn active_layer_filter_params(&self, filter_index: usize) -> Option<Vec<ParamSpec>> {
        self.active_layer()
            .and_then(|l| l.filters.get(filter_index))
            .map(|f| f.params())
    }

    pub fn canvas(&self) -> &CanvasInfo {
        &self.document.canvas
    }
}
