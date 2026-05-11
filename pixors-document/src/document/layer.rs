use pixors_image::image::BlendMode;
use serde::{Deserialize, Serialize};

use super::NodeId;
use super::transform::Transform;

/// A single layer in the document's layer stack.
/// Flat list for now — groups come later.
/// Order: index 0 = bottommost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerNode {
    pub id: NodeId,
    pub name: String,
    pub visible: bool,
    pub blend: BlendSpec,
    pub source: PixelSource,
    /// Ordered transform stack applied before compositing.
    pub transforms: Vec<Transform>,
    /// Slot for future mask.
    pub mask: Option<Mask>,
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
