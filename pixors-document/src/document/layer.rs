use pixors_image::image::BlendMode;
use serde::{Deserialize, Serialize};

use super::NodeId;

/// A single layer in the document's layer stack.
/// Flat list for Phase 10 — groups come later.
/// Order: index 0 = bottommost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerNode {
    pub id: NodeId,
    pub name: String,
    pub visible: bool,
    pub blend: BlendSpec,
    pub source: PixelSource,
    /// Per-layer ordered filter stack. Applies before compositing.
    pub filters: Vec<LayerFilter>,
    /// Slot for future mask.
    pub mask: Option<Mask>,
}

/// Non-destructive per-layer filter.
/// Public-facing filter type. Only the `document` module pattern-matches on variants.
/// Desktop code uses `params()` / `label()` / `set_param()` — generic rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayerFilter {
    Blur { radius: f32 },
}

impl LayerFilter {
    pub fn label(&self) -> &str {
        match self {
            LayerFilter::Blur { .. } => "Gaussian Blur",
        }
    }

    pub fn params(&self) -> Vec<crate::view::params::ParamSpec> {
        match self {
            LayerFilter::Blur { radius } => vec![
                crate::view::params::ParamSpec::float("radius", "Radius", *radius, 0.0..=64.0),
            ],
        }
    }

    pub fn set_param(&mut self, name: &str, value: &crate::view::params::ParamValue) -> bool {
        match (self, name, value) {
            (LayerFilter::Blur { radius }, "radius", crate::view::params::ParamValue::F32(v)) => {
                *radius = *v;
                true
            }
            _ => false,
        }
    }
}

/// Blend mode + opacity for a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendSpec {
    pub mode: BlendMode,
    pub opacity: f32, // 0.0..=1.0
}

impl Default for BlendSpec {
    fn default() -> Self {
        Self {
            mode: BlendMode::Normal,
            opacity: 1.0,
        }
    }
}

/// Source of pixel data for a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PixelSource {
    /// Page of the primary asset (multi-page TIFF, etc.)
    PrimaryAsset { page: usize },
    /// Solid color fill.
    SolidColor { color: [u8; 4] },
}

/// Slot for future mask implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mask {
    pub _reserved: (),
}
