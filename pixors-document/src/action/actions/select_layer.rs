use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::{EditorState, NodeId, TabId};

/// Select (activate) a layer in the layers panel. Not undoable.
#[derive(Debug)]
pub struct SelectLayer {
    pub tab: TabId,
    pub layer: NodeId,
}

impl Action for SelectLayer {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, state: &mut EditorState, _status: PipelineStatus) {
        if let Some(tab) = state.tab_mut(self.tab) {
            tab.session.active_node = Some(self.layer);
        }
    }

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
