use serde::{Deserialize, Serialize};

use pixors_image::image::BlendMode;

use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::document::{Document, LayerNode, NodeId};
use crate::document::transform::{Operation, Transform};
use crate::{EditorState, TabId};

use super::DocumentMutation;

// ── Dual-trait helper macro ───────────────────────────────────────────
//
// Generates `impl Action for T` for a DocumentMutation.
// Uses: impl_document_action!(T, tab_field);
macro_rules! impl_document_action {
    ($ty:ty, $tab_field:ident) => {
        impl Action for $ty {
            fn target_tab(&self) -> Option<TabId> { Some(self.$tab_field) }
            fn prepare(&self, _: &mut EditorState) -> Result<PreparedAction, String> { Ok(PreparedAction::StateOnly) }
            fn apply(&self, state: &mut EditorState, _: PipelineStatus) {
                if let Some(tab) = state.tab_mut(self.$tab_field) {
                    tab.history.push(std::sync::Arc::new(self.clone()), &mut tab.document);
                    tab.session.redraw_seq += 1;
                }
            }
            // Undo for document mutations is driven by UndoAction → History::undo,
            // not by Action::undo. This path should never be called directly.
            fn undo(&self, _: &mut EditorState) {}
            fn record_in_history(&self) -> bool { false }
        }
    };
}
// ── SetLayerName ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerName {
    pub tab: TabId,
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
impl_document_action!(SetLayerName, tab);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerVisible {
    pub tab: TabId,
    pub layer: NodeId,
    pub before: bool,
    pub after: bool,
}
#[typetag::serde]
impl DocumentMutation for SetLayerVisible {
    fn apply(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.visible = self.after; } }
    fn undo(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.visible = self.before; } }
    fn label(&self) -> &str { if self.after { "Show Layer" } else { "Hide Layer" } }
}
impl_document_action!(SetLayerVisible, tab);
// SetLayerOpacity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerOpacity {
    pub tab: TabId,
    pub layer: NodeId,
    pub before: f32,
    pub after: f32,
}
#[typetag::serde]
impl DocumentMutation for SetLayerOpacity {
    fn apply(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.opacity = self.after; } }
    fn undo(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.opacity = self.before; } }
    fn label(&self) -> &str { "Set Opacity" }
}
impl_document_action!(SetLayerOpacity, tab);
// SetLayerBlend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerBlend {
    pub tab: TabId,
    pub layer: NodeId,
    pub before: BlendMode,
    pub after: BlendMode,
}
#[typetag::serde]
impl DocumentMutation for SetLayerBlend {
    fn apply(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.mode = self.after; } }
    fn undo(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.blend.mode = self.before; } }
    fn label(&self) -> &str { "Set Blend Mode" }
}
impl_document_action!(SetLayerBlend, tab);
// AddLayer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLayer {
    pub tab: TabId,
    pub at_index: usize,
    pub layer: LayerNode,
}
#[typetag::serde]
impl DocumentMutation for AddLayer {
    fn apply(&self, doc: &mut Document) {
        let idx = self.at_index.min(doc.layers.len());
        doc.layers.insert(idx, self.layer.clone());
    }
    fn undo(&self, doc: &mut Document) {
        if self.at_index < doc.layers.len() {
            doc.layers.remove(self.at_index);
        }
    }
    fn label(&self) -> &str { "Add Layer" }
}
impl_document_action!(AddLayer, tab);
// RemoveLayer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLayer {
    pub tab: TabId,
    pub index: usize,
    pub layer: LayerNode,
}
#[typetag::serde]
impl DocumentMutation for RemoveLayer {
    fn apply(&self, doc: &mut Document) {
        if self.index < doc.layers.len() {
            doc.layers.remove(self.index);
        }
    }
    fn undo(&self, doc: &mut Document) {
        let idx = self.index.min(doc.layers.len());
        doc.layers.insert(idx, self.layer.clone());
    }
    fn label(&self) -> &str { "Remove Layer" }
}
impl_document_action!(RemoveLayer, tab);
// SwapLayers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapLayers {
    pub tab: TabId,
    pub index_a: usize,
    pub index_b: usize,
}
#[typetag::serde]
impl DocumentMutation for SwapLayers {
    fn apply(&self, doc: &mut Document) { doc.layers.swap(self.index_a, self.index_b); }
    fn undo(&self, doc: &mut Document) { doc.layers.swap(self.index_a, self.index_b); }
    fn label(&self) -> &str { "Reorder Layers" }
}
impl_document_action!(SwapLayers, tab);
// AddTransform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTransform {
    pub tab: TabId,
    pub layer: NodeId,
    pub transform: Transform,
}
#[typetag::serde]
impl DocumentMutation for AddTransform {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.transforms.push(self.transform.clone());
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.transforms.retain(|t| t.id != self.transform.id);
        }
    }
    fn label(&self) -> &str { "Add Transform" }
}
impl_document_action!(AddTransform, tab);
// RemoveTransform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveTransform {
    pub tab: TabId,
    pub layer: NodeId,
    pub transform_id: NodeId,
    pub removed: Transform,
    pub index: usize,
}
#[typetag::serde]
impl DocumentMutation for RemoveTransform {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && let Some(i) = l.transforms.iter().position(|t| t.id == self.transform_id)
        {
            l.transforms.remove(i);
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            let idx = self.index.min(l.transforms.len());
            l.transforms.insert(idx, self.removed.clone());
        }
    }
    fn label(&self) -> &str { "Remove Transform" }
}
impl_document_action!(RemoveTransform, tab);
// UpdateTransformOp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTransformOp {
    pub tab: TabId,
    pub layer: NodeId,
    pub transform_id: NodeId,
    pub before: Operation,
    pub after: Operation,
}
#[typetag::serde]
impl DocumentMutation for UpdateTransformOp {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && let Some(t) = l.transforms.iter_mut().find(|t| t.id == self.transform_id)
        {
            t.op = self.after.clone();
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && let Some(t) = l.transforms.iter_mut().find(|t| t.id == self.transform_id)
        {
            t.op = self.before.clone();
        }
    }
    fn label(&self) -> &str { "Update Transform" }
}
impl_document_action!(UpdateTransformOp, tab);
