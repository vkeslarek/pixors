pub mod asset;
pub mod cache;
pub mod canvas;
pub mod develop;
pub mod layer;
pub mod transform;

pub use asset::{AssetId, AssetStore};
pub use canvas::CanvasInfo;
pub use develop::{DevelopAdjustment, DevelopState};
pub use layer::{BlendSpec, LayerNode, Mask, PixelSource};
pub use transform::{CompositePosition, InputScope, Operation, OutputMode, Transform};

use serde::{Deserialize, Serialize};

/// Stable across document edits. Incrementing u64, scoped per Document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// The canonical, serializable project state.
/// Everything that goes to disk on Save. No UI fields, no GPU handles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub canvas: CanvasInfo,
    pub assets: AssetStore,
    pub develop: DevelopState,
    /// Flat layer stack. Index 0 = bottommost.
    pub layers: Vec<LayerNode>,
    next_node_id: u64,
    next_asset_id: u64,
}

impl Document {
    pub fn new(canvas: CanvasInfo) -> Self {
        Self {
            canvas,
            assets: AssetStore::default(),
            develop: DevelopState::default(),
            layers: Vec::new(),
            next_node_id: 0,
            next_asset_id: 0,
        }
    }

    pub fn alloc_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn alloc_asset_id(&mut self) -> AssetId {
        let id = AssetId(self.next_asset_id);
        self.next_asset_id += 1;
        id
    }

    pub fn find_layer(&self, id: NodeId) -> Option<&LayerNode> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn find_layer_mut(&mut self, id: NodeId) -> Option<&mut LayerNode> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn visible_layers(&self) -> Vec<&LayerNode> {
        self.layers.iter().filter(|l| l.visible).collect()
    }
}
