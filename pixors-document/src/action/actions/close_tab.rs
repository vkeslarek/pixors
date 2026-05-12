use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::{EditorState, SessionId};

#[derive(Debug)]
pub struct CloseTab(pub SessionId);

impl Action for CloseTab {
    fn target_tab(&self) -> Option<SessionId> {
        Some(self.0)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        state.close(self.0);
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
