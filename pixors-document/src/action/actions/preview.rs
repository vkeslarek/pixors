use std::collections::HashMap;

use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::session::PreviewState;
use crate::view::params::ParamValue;
use crate::{EditorState, TabId};

/// Update the live preview for one transform parameter. Does not touch History.
#[derive(Debug)]
pub struct UpdatePreview {
    pub tab: TabId,
    pub layer_id: crate::document::NodeId,
    pub filter_index: usize,
    pub param: String,
    pub value: ParamValue,
    pub preview_mip: u32,
}

impl Action for UpdatePreview {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab) {
            let needs_reset = tab.session.active_preview.as_ref().is_some_and(|p| {
                p.layer_id != self.layer_id || p.filter_index != self.filter_index
            });
            if needs_reset {
                tab.session.active_preview = None;
            }
            let preview = tab
                .session
                .active_preview
                .get_or_insert_with(|| PreviewState {
                    layer_id: self.layer_id,
                    filter_index: self.filter_index,
                    overrides: HashMap::new(),
                    preview_mip: self.preview_mip,
                });
            preview
                .overrides
                .insert(self.param.clone(), self.value.clone());
        }
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _: &mut EditorState, _: PipelineStatus) {}
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool {
        false
    }
}

/// Commit the current preview overrides as real document mutations.
/// Currently a no-op — commit is handled via Transform mutations in the desktop controller.
#[derive(Debug)]
pub struct CommitPreview {
    pub tab: TabId,
}

impl Action for CommitPreview {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.session.active_preview = None;
            tab.session.redraw_seq += 1;
        }
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _: &mut EditorState, _: PipelineStatus) {}
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool {
        false
    }
}

/// Discard preview, revert to document state.
#[derive(Debug)]
pub struct CancelPreview {
    pub tab: TabId,
}

impl Action for CancelPreview {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.session.active_preview = None;
        }
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _: &mut EditorState, _: PipelineStatus) {}
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool {
        false
    }
}
