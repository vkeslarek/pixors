use serde::{Deserialize, Serialize};

use pixors_image::image::BlendMode;

use crate::SessionId;
use crate::document::transform::{Operation, Transform};
use crate::document::{Document, LayerNode, NodeId};

use super::Mutation;

// ── SetLayerName ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerName {
    pub tab: SessionId,
    pub layer: NodeId,
    pub before: String,
    pub after: String,
}
#[typetag::serde]
impl Mutation for SetLayerName {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Rename Layer"
    }
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.name = self.after.clone();
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.name = self.before.clone();
        }
    }
}

// ── SetLayerVisible ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerVisible {
    pub tab: SessionId,
    pub layer: NodeId,
    pub before: bool,
    pub after: bool,
}
#[typetag::serde]
impl Mutation for SetLayerVisible {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        if self.after {
            "Show Layer"
        } else {
            "Hide Layer"
        }
    }
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.visible = self.after;
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.visible = self.before;
        }
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── SetLayerOpacity ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerOpacity {
    pub tab: SessionId,
    pub layer: NodeId,
    pub before: f32,
    pub after: f32,
}
#[typetag::serde]
impl Mutation for SetLayerOpacity {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Set Opacity"
    }
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.blend.opacity = self.after;
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.blend.opacity = self.before;
        }
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── SetLayerBlend ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerBlend {
    pub tab: SessionId,
    pub layer: NodeId,
    pub before: BlendMode,
    pub after: BlendMode,
}
#[typetag::serde]
impl Mutation for SetLayerBlend {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Set Blend Mode"
    }
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.blend.mode = self.after;
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) {
            l.blend.mode = self.before;
        }
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── AddLayer ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLayer {
    pub tab: SessionId,
    pub at_index: usize,
    pub layer: LayerNode,
}
#[typetag::serde]
impl Mutation for AddLayer {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Add Layer"
    }
    fn apply(&self, doc: &mut Document) {
        let idx = self.at_index.min(doc.layers.len());
        doc.layers.insert(idx, self.layer.clone());
    }
    fn undo(&self, doc: &mut Document) {
        if self.at_index < doc.layers.len() {
            doc.layers.remove(self.at_index);
        }
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── RemoveLayer ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLayer {
    pub tab: SessionId,
    pub index: usize,
    pub layer: LayerNode,
}
#[typetag::serde]
impl Mutation for RemoveLayer {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Remove Layer"
    }
    fn apply(&self, doc: &mut Document) {
        if self.index < doc.layers.len() {
            doc.layers.remove(self.index);
        }
    }
    fn undo(&self, doc: &mut Document) {
        let idx = self.index.min(doc.layers.len());
        doc.layers.insert(idx, self.layer.clone());
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── SwapLayers ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapLayers {
    pub tab: SessionId,
    pub index_a: usize,
    pub index_b: usize,
}
#[typetag::serde]
impl Mutation for SwapLayers {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Reorder Layers"
    }
    fn apply(&self, doc: &mut Document) {
        doc.layers.swap(self.index_a, self.index_b);
    }
    fn undo(&self, doc: &mut Document) {
        doc.layers.swap(self.index_a, self.index_b);
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── AddTransform ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTransform {
    pub tab: SessionId,
    pub layer: NodeId,
    pub transform: Transform,
}
#[typetag::serde]
impl Mutation for AddTransform {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Add Transform"
    }
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
    fn preview_op(&self) -> Option<Operation> {
        Some(self.transform.op.clone())
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── RemoveTransform ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveTransform {
    pub tab: SessionId,
    pub layer: NodeId,
    pub transform_id: NodeId,
    pub removed: Transform,
    pub index: usize,
}
#[typetag::serde]
impl Mutation for RemoveTransform {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Remove Transform"
    }
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
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── UpdateTransformOp ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTransformOp {
    pub tab: SessionId,
    pub layer: NodeId,
    pub transform_id: NodeId,
    pub before: Operation,
    pub after: Operation,
}
#[typetag::serde]
impl Mutation for UpdateTransformOp {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Update Transform"
    }
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
    fn preview_op(&self) -> Option<Operation> {
        Some(self.after.clone())
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── SetTransformEnabled ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTransformEnabled {
    pub tab: SessionId,
    pub layer: NodeId,
    pub transform_id: NodeId,
    pub before: bool,
    pub after: bool,
}
#[typetag::serde]
impl Mutation for SetTransformEnabled {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        if self.after {
            "Enable Filter"
        } else {
            "Disable Filter"
        }
    }
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && let Some(t) = l.transforms.iter_mut().find(|t| t.id == self.transform_id)
        {
            t.enabled = self.after;
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && let Some(t) = l.transforms.iter_mut().find(|t| t.id == self.transform_id)
        {
            t.enabled = self.before;
        }
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}

// ── ReorderTransform ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderTransform {
    pub tab: SessionId,
    pub layer: NodeId,
    pub from: usize,
    pub to: usize,
}
#[typetag::serde]
impl Mutation for ReorderTransform {
    fn target_session(&self) -> SessionId {
        self.tab
    }
    fn label(&self) -> &str {
        "Reorder Filter"
    }
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && self.from < l.transforms.len()
            && self.to < l.transforms.len()
        {
            let t = l.transforms.remove(self.from);
            l.transforms.insert(self.to, t);
        }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && self.from < l.transforms.len()
            && self.to < l.transforms.len()
            && self.from != self.to
        {
            let t = l.transforms.remove(self.to);
            l.transforms.insert(self.from, t);
        }
    }
    fn needs_recompile(&self) -> bool {
        true
    }
}
