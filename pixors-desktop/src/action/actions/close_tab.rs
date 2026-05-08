use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::state::{EditorState, TabId};

#[derive(Debug)]
pub struct CloseTab(pub TabId);

impl Action for CloseTab {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.0)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        pixors_executor::sink::viewport_cache_sink::unregister_tab_cache(self.0.0);
        pixors_executor::source::viewport_cache_source::uninstall_viewport_cache_reader(self.0.0);
        state.close(self.0);
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
