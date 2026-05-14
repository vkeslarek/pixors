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
                self.pending_ingest = None;
                self.dispatcher
                    .on_pipeline_done(&mut self.state, session_id);

                // If the viewport tab doesn't exist yet, this is the ingest pipeline finishing.
                // Start the render pipeline and keep loading=true so the spinner continues to
                // drive redraws until tiles drain from TileCache into the GPU atlas.
                let is_ingest_done = self.state.session(session_id).is_some()
                    && !self.viewport_tabs.contains_key(&session_id);

                if is_ingest_done {
                    self.init_viewport_for_tab(session_id);
                    // loading stays true; tick clears it when has_pending() is false.
                    if let Some(tab) = self.state.session_mut(session_id) {
                        tab.transient.view.progress = 1.0;
                    }
                } else if let Some(tab) = self.state.session_mut(session_id) {
                    tab.transient.view.progress = 1.0;
                    tab.transient.view.loading = false;
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
                // A new pipeline (e.g. export) may have already started and re-set loading=true.
                // Only clear if nothing else is running.
                if !self.dispatcher.is_background_running(session_id) {
                    if let Some(tab) = self.state.session_mut(session_id) {
                        tab.transient.view.loading = false;
                    }
                }
            }
        }
    }
}
