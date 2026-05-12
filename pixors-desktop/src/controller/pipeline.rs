use pixors_document::TabId;
use pixors_engine::runtime::event::PipelineEvent;

use crate::app::App;

impl App {
    pub(crate) fn handle_pipeline_event(&mut self, e: PipelineEvent) {
        match e {
            PipelineEvent::Progress { tag, done, total } => {
                let p = if total > 0 {
                    done as f32 / total as f32
                } else {
                    1.0
                };
                let tab_id = TabId(tag);
                if let Some(tab) = self.state.tab_mut(tab_id) {
                    tab.session.view.progress = p;
                }
            }
            PipelineEvent::Done { tag } => {
                let tab_id = TabId(tag);
                self.dispatcher.on_pipeline_done(&mut self.state, tab_id);
                if let Some(tab) = self.state.tab_mut(tab_id) {
                    tab.session.view.loading = false;
                    tab.session.view.progress = 1.0;
                }
                if self.state.tab(tab_id).is_some() && !self.viewport_tabs.contains_key(&tab_id) {
                    self.init_viewport_for_tab(tab_id);
                }
            }
            PipelineEvent::Error { tag, message } => {
                let tab_id = TabId(tag);
                self.dispatcher
                    .on_pipeline_error(&mut self.state, tab_id, message.clone());
                if let Some(tab) = self.state.tab_mut(tab_id) {
                    tab.session.view.loading = false;
                }
                self.push_error(message);
            }
            PipelineEvent::Cancelled { tag } => {
                let tab_id = TabId(tag);
                if let Some(tab) = self.state.tab_mut(tab_id) {
                    tab.session.view.loading = false;
                }
            }
        }
    }
}
