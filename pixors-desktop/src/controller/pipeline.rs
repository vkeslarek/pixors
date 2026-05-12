use pixors_document::SessionId;
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
                let session_id = SessionId(tag);
                if let Some(tab) = self.state.session_mut(session_id) {
                    tab.transient.view.progress = p;
                }
            }
            PipelineEvent::Done { tag } => {
                let session_id = SessionId(tag);
                // Push pending ingest session if this was an OpenFile pipeline
                if let Some(session) = self.pending_ingest.take()
                    && session.id == session_id
                {
                    self.state.push(session);
                }
                self.dispatcher
                    .on_pipeline_done(&mut self.state, session_id);
                if let Some(tab) = self.state.session_mut(session_id) {
                    tab.transient.view.loading = false;
                    tab.transient.view.progress = 1.0;
                }
                if self.state.session(session_id).is_some()
                    && !self.viewport_tabs.contains_key(&session_id)
                {
                    self.init_viewport_for_tab(session_id);
                }
            }
            PipelineEvent::Error { tag, message } => {
                let session_id = SessionId(tag);
                self.dispatcher
                    .on_pipeline_error(&mut self.state, session_id, message.clone());
                if let Some(tab) = self.state.session_mut(session_id) {
                    tab.transient.view.loading = false;
                }
                self.push_error(message);
            }
            PipelineEvent::Cancelled { tag } => {
                let session_id = SessionId(tag);
                if let Some(tab) = self.state.session_mut(session_id) {
                    tab.transient.view.loading = false;
                }
            }
        }
    }
}
