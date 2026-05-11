use serde::{Deserialize, Serialize};

use pixors_image::image::BlendMode;

use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::document::{Document, LayerFilter, LayerNode, NodeId};
use crate::view::params::ParamValue;
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
            fn undo(&self, state: &mut EditorState) {
                if let Some(tab) = state.tab_mut(self.$tab_field) {
                    tab.history.undo(&mut tab.document);
                    tab.session.redraw_seq += 1;
                }
            }
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
    fn apply(&self, doc: &mut Document) { doc.layers.insert(self.at_index, self.layer.clone()); }
    fn undo(&self, doc: &mut Document) { doc.layers.remove(self.at_index); }
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
    fn apply(&self, doc: &mut Document) { doc.layers.remove(self.index); }
    fn undo(&self, doc: &mut Document) { doc.layers.insert(self.index, self.layer.clone()); }
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
// AddLayerFilter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLayerFilter {
    pub tab: TabId,
    pub layer: NodeId,
    pub at_index: usize,
    pub filter: LayerFilter,
}
#[typetag::serde]
impl DocumentMutation for AddLayerFilter {
    fn apply(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.filters.insert(self.at_index, self.filter.clone()); } }
    fn undo(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.filters.remove(self.at_index); } }
    fn label(&self) -> &str { "Add Filter" }
}
impl_document_action!(AddLayerFilter, tab);
// RemoveLayerFilter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLayerFilter {
    pub tab: TabId,
    pub layer: NodeId,
    pub index: usize,
    pub filter: LayerFilter,
}
#[typetag::serde]
impl DocumentMutation for RemoveLayerFilter {
    fn apply(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.filters.remove(self.index); } }
    fn undo(&self, doc: &mut Document) { if let Some(l) = doc.find_layer_mut(self.layer) { l.filters.insert(self.index, self.filter.clone()); } }
    fn label(&self) -> &str { "Remove Filter" }
}
impl_document_action!(RemoveLayerFilter, tab);
// SetFilterParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetFilterParam {
    pub tab: TabId,
    pub layer: NodeId,
    pub filter_index: usize,
    pub param: String,
    pub before: ParamValue,
    pub after: ParamValue,
}
#[typetag::serde]
impl DocumentMutation for SetFilterParam {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) && let Some(f) = l.filters.get_mut(self.filter_index) { f.set_param(&self.param, &self.after); }
    }
    fn undo(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer) && let Some(f) = l.filters.get_mut(self.filter_index) { f.set_param(&self.param, &self.before); }
    }
    fn label(&self) -> &str { "Adjust Parameter" }
}
impl_document_action!(SetFilterParam, tab);
