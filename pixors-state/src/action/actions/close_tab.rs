use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::{EditorState, TabId};

#[derive(Debug)]
pub struct CloseTab(pub TabId);

impl Action for CloseTab {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.0)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        crate::tile_cache_sink::unregister_tile_cache(self.0.0);
        crate::tile_cache_source::uninstall_tile_cache_reader(self.0.0);
        state.close(self.0);
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
