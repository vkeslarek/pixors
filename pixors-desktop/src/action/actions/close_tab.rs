use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::state::{EditorState, TabId};

#[derive(Debug)]
pub struct CloseTab(pub TabId);

impl Action for CloseTab {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.0)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        // Release global sink so the closed tab's ViewportCache is freed
        pixors_executor::sink::viewport_cache_sink::uninstall_viewport_cache_sink();
        state.close(self.0);
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
