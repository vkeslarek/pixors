use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::state::{EditorState, TabId};

#[derive(Debug)]
pub struct SwitchTab(pub TabId);

impl Action for SwitchTab {
    fn target_tab(&self) -> Option<TabId> {
        None
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        state.switch(self.0);
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
