use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::sync_channel;
use std::thread;

use pixors_engine::graph::graph::ExecGraph;
use pixors_engine::runtime::event::PipelineEvent;
use pixors_engine::runtime::pipeline::{Pipeline, PipelineHandle};
use tokio::sync::broadcast;

use crate::document::Operation;
use crate::mutation::Mutation;
use crate::{EditorState, SessionId};

pub struct Dispatcher {
    pub event_tx: broadcast::Sender<PipelineEvent>,
    background_tasks: HashMap<SessionId, PipelineHandle>,
}

impl Dispatcher {
    pub fn new(event_tx: broadcast::Sender<PipelineEvent>) -> Self {
        Self {
            event_tx,
            background_tasks: HashMap::new(),
        }
    }

    pub fn run_graph(
        &mut self,
        graph: ExecGraph,
        session_id: Option<SessionId>,
    ) -> Result<(), String> {
        let tag = session_id.map(|t| t.0).unwrap_or(0);

        let cancelled = Arc::new(AtomicBool::new(false));
        let (event_tx, event_rx) = sync_channel::<PipelineEvent>(64);
        let pipeline = Pipeline::compile(graph, Some(event_tx.clone()), cancelled.clone(), tag)
            .map_err(|e| e.to_string())?;

        let broadcast_tx = self.event_tx.clone();
        thread::spawn(move || {
            while let Ok(event) = event_rx.recv() {
                let tagged = match event {
                    PipelineEvent::Error { message, .. } => PipelineEvent::Error { tag, message },
                    PipelineEvent::Cancelled { .. } => PipelineEvent::Cancelled { tag },
                    PipelineEvent::Progress { done, total, .. } => {
                        PipelineEvent::Progress { tag, done, total }
                    }
                    other => other,
                };
                let _ = broadcast_tx.send(tagged);
            }
            let _ = broadcast_tx.send(PipelineEvent::Done { tag });
        });

        let handle = pipeline.run(Some(event_tx));

        if let Some(tid) = session_id {
            self.background_tasks.insert(tid, handle);
        }

        Ok(())
    }

    pub fn on_pipeline_done(&mut self, _state: &mut EditorState, _session_id: SessionId) {}

    pub fn on_pipeline_error(
        &mut self,
        _state: &mut EditorState,
        _session_id: SessionId,
        _error: String,
    ) {
    }

    pub fn cleanup_tab(&mut self, id: SessionId) {
        self.background_tasks.remove(&id);
    }

    pub fn cancel_background(&mut self, id: SessionId) {
        if let Some(handle) = self.background_tasks.remove(&id) {
            handle.cancel();
        }
    }

    pub fn resync_locks(&mut self, state: &mut EditorState) {
        self.background_tasks.retain(|session_id, handle| {
            let still_running = handle.is_running();
            if !still_running && let Some(tab) = state.session_mut(*session_id) {
                tab.transient.view.loading = false;
                tab.transient.view.progress = 1.0;
            }
            still_running
        });
    }

    pub fn commit(
        &mut self,
        mutation: Arc<dyn Mutation>,
        state: &mut EditorState,
    ) -> Result<(), String> {
        let session_id = mutation.target_session();
        let session = state.session_mut(session_id).ok_or("session not found")?;

        if mutation.recordable() {
            session
                .history
                .push(mutation.clone(), &mut session.document);
        }
        session.transient.redraw_seq += 1;

        Ok(())
    }

    pub fn preview_op(&self, mutation: &dyn Mutation) -> Option<Operation> {
        mutation.preview_op()
    }
}
