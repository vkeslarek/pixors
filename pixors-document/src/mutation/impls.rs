use serde::{Deserialize, Serialize};

use pixors_image::image::BlendMode;

use crate::document::{Document, LayerFilter, LayerNode, NodeId};
use crate::view::params::ParamValue;

use super::DocumentMutation;

// ── Layer mutations ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerName {
    pub layer: NodeId,
    pub before: String,
    pub after: String,
}

#[typetag::serde]
impl DocumentMutation for SetLayerName {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.name = self.after.clone(); }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.name = self.before.clone(); }
    }
    fn label(&self) -> &str { "Rename Layer" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerVisible {
    pub layer: NodeId,
    pub before: bool,
    pub after: bool,
}

#[typetag::serde]
impl DocumentMutation for SetLayerVisible {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.visible = self.after; }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.visible = self.before; }
    }
    fn label(&self) -> &str { if self.after { "Show Layer" } else { "Hide Layer" } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerOpacity {
    pub layer: NodeId,
    pub before: f32,
    pub after: f32,
}

#[typetag::serde]
impl DocumentMutation for SetLayerOpacity {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.opacity = self.after; }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.opacity = self.before; }
    }
    fn label(&self) -> &str { "Set Opacity" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerBlend {
    pub layer: NodeId,
    pub before: BlendMode,
    pub after: BlendMode,
}

#[typetag::serde]
impl DocumentMutation for SetLayerBlend {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.mode = self.after; }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.mode = self.before; }
    }
    fn label(&self) -> &str { "Set Blend Mode" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLayer {
    pub at_index: usize,
    pub layer: LayerNode,
}

#[typetag::serde]
impl DocumentMutation for AddLayer {
    fn apply(&self, doc: &mut Document) {
        doc.layers.insert(self.at_index, self.layer.clone());
    }
    fn undo(&self, doc: &mut Document) {
        doc.layers.remove(self.at_index);
    }
    fn label(&self) -> &str { "Add Layer" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLayer {
    pub index: usize,
    pub layer: LayerNode,
}

#[typetag::serde]
impl DocumentMutation for RemoveLayer {
    fn apply(&self, doc: &mut Document) {
        doc.layers.remove(self.index);
    }
    fn undo(&self, doc: &mut Document) {
        doc.layers.insert(self.index, self.layer.clone());
    }
    fn label(&self) -> &str { "Remove Layer" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapLayers {
    pub index_a: usize,
    pub index_b: usize,
}

#[typetag::serde]
impl DocumentMutation for SwapLayers {
    fn apply(&self, doc: &mut Document) {
        doc.layers.swap(self.index_a, self.index_b);
    }
    fn undo(&self, doc: &mut Document) {
        doc.layers.swap(self.index_a, self.index_b);
    }
    fn label(&self) -> &str { "Reorder Layers" }
}

// ── Filter mutations ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLayerFilter {
    pub layer: NodeId,
    pub at_index: usize,
    pub filter: LayerFilter,
}

#[typetag::serde]
impl DocumentMutation for AddLayerFilter {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.filters.insert(self.at_index, self.filter.clone());
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.filters.remove(self.at_index);
        }
    }
    fn label(&self) -> &str { "Add Filter" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLayerFilter {
    pub layer: NodeId,
    pub index: usize,
    pub filter: LayerFilter,
}

#[typetag::serde]
impl DocumentMutation for RemoveLayerFilter {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.filters.remove(self.index);
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.filters.insert(self.index, self.filter.clone());
        }
    }
    fn label(&self) -> &str { "Remove Filter" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetFilterParam {
    pub layer: NodeId,
    pub filter_index: usize,
    pub param: String,
    pub before: ParamValue,
    pub after: ParamValue,
}

#[typetag::serde]
impl DocumentMutation for SetFilterParam {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            if let Some(f) = l.filters.get_mut(self.filter_index) {
                f.set_param(&self.param, &self.after);
            }
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            if let Some(f) = l.filters.get_mut(self.filter_index) {
                f.set_param(&self.param, &self.before);
            }
        }
    }
    fn label(&self) -> &str { "Adjust Parameter" }
}
