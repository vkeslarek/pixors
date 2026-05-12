use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::{EditorState, NodeId, SessionId};

/// Select (activate) a layer in the layers panel. Not undoable.
#[derive(Debug)]
pub struct SelectLayer {
    pub tab: SessionId,
    pub layer: NodeId,
}

impl Action for SelectLayer {
    fn target_tab(&self) -> Option<SessionId> {
        Some(self.tab)
    }

    fn prepare(&self, _state: &mut EditorState) -> Result<PreparedAction, String> {
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, state: &mut EditorState, _status: PipelineStatus) {
        if let Some(tab) = state.session_mut(self.tab) {
            tab.transient.active_node = Some(self.layer);
        }
    }

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
