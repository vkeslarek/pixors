use crate::action::{Action, PipelineStatus, PreparedAction};
use crate::state::{EditorState, TabId};

#[derive(Debug)]
pub struct BlurCancel {
    pub tab: TabId,
    pub generation: u64,
}

impl Action for BlurCancel {
    fn target_tab(&self) -> Option<TabId> {
        Some(self.tab)
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String> {
        if let Some(tab) = state.tab_mut(self.tab)
            && let Ok(mut cache) = tab.viewport_cache.lock()
        {
            cache.clear_generation(self.generation);
        }
        Ok(PreparedAction::StateOnly)
    }

    fn apply(&self, _state: &mut EditorState, _status: PipelineStatus) {}

    fn undo(&self, _state: &mut EditorState) {}

    fn record_in_history(&self) -> bool {
        false
    }
}
