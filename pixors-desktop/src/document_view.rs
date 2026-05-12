#![allow(dead_code)]

use pixors_document::Transient;
use pixors_document::document::canvas::CanvasInfo;
use pixors_document::document::layer::{LayerNode, PixelSource};
use pixors_document::document::transform::Transform;
use pixors_document::document::{Document, NodeId};
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
    pub depth: u8,
    pub transform_count: usize,
    pub has_mask: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Pixel,
    SolidColor,
}

// ── ParamSpec ───────────────────────────────────────────────────────────

/// Describes one editable parameter of a filter, ready for generic UI rendering.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    Float {
        value: f32,
        range: std::ops::RangeInclusive<f32>,
    },
    Int {
        value: i32,
        range: std::ops::RangeInclusive<i32>,
    },
    Bool {
        value: bool,
    },
}

impl ParamSpec {
    pub fn float(
        name: &'static str,
        label: &'static str,
        value: f32,
        range: std::ops::RangeInclusive<f32>,
    ) -> Self {
        Self {
            name,
            label,
            kind: ParamKind::Float { value, range },
        }
    }
    pub fn int(
        name: &'static str,
        label: &'static str,
        value: i32,
        range: std::ops::RangeInclusive<i32>,
    ) -> Self {
        Self {
            name,
            label,
            kind: ParamKind::Int { value, range },
        }
    }
    pub fn bool(name: &'static str, label: &'static str, value: bool) -> Self {
        Self {
            name,
            label,
            kind: ParamKind::Bool { value },
        }
    }
}

// ── DocumentView ────────────────────────────────────────────────────────

/// Derived, widget-ready view of a Document + Transient.
/// No internal cache — computed eagerly.
pub struct DocumentView<'a> {
    document: &'a Document,
    transient: &'a Transient,
}

impl<'a> DocumentView<'a> {
    pub fn new(document: &'a Document, transient: &'a Transient) -> Self {
        Self {
            document,
            transient,
        }
    }

    pub fn layers_panel(&self) -> Vec<LayerPanelItem> {
        self.document
            .layers
            .iter()
            .map(|l| LayerPanelItem {
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
                transform_count: l.transforms.len(),
                has_mask: l.mask.is_some(),
            })
            .collect()
    }

    pub fn active_layer(&self) -> Option<&LayerNode> {
        self.transient
            .active_node
            .and_then(|id| self.document.find_layer(id))
    }

    pub fn active_layer_transforms(&self) -> Option<&[Transform]> {
        self.active_layer().map(|l| l.transforms.as_slice())
    }

    pub fn active_layer_transform_params(&self, transform_index: usize) -> Option<Vec<ParamSpec>> {
        self.active_layer()
            .and_then(|l| l.transforms.get(transform_index))
            .map(transform_params)
    }

    pub fn canvas(&self) -> &CanvasInfo {
        &self.document.canvas
    }
}

fn transform_params(t: &Transform) -> Vec<ParamSpec> {
    use pixors_document::document::transform::Operation;
    match &t.op {
        Operation::Blur { radius } => {
            vec![ParamSpec::float("radius", "Radius", *radius, 0.0..=64.0)]
        }
        Operation::Exposure { stops } => {
            vec![ParamSpec::float("stops", "Stops", *stops, -5.0..=5.0)]
        }
    }
}
