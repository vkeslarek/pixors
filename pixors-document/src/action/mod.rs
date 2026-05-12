pub mod actions;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::sync_channel;
use std::thread;

use pixors_engine::graph::graph::ExecGraph;
use pixors_engine::runtime::event::PipelineEvent;
use pixors_engine::runtime::pipeline::{Pipeline, PipelineHandle};
use tokio::sync::broadcast;

use crate::{EditorState, SessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Background,
    Apply,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PipelineStatus {
    Done,
    Error(String),
    Cancelled,
}

#[allow(dead_code)]
pub enum PreparedAction {
    StateOnly,
    Pipeline {
        mode: PipelineMode,
        graph: ExecGraph,
        routed_tab: Option<SessionId>,
    },
}

#[allow(dead_code)]
pub trait Action: std::fmt::Debug + Send + Sync + 'static {
    fn target_tab(&self) -> Option<SessionId> {
        None
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String>;

    fn apply(&self, state: &mut EditorState, status: PipelineStatus);

    fn undo(&self, state: &mut EditorState);

    fn record_in_history(&self) -> bool {
        true
    }
}

/// Typed chain of actions. Replaces raw `Vec<Arc<dyn Action>>` at callsites.
pub struct ActionChain {
    actions: Vec<Arc<dyn Action>>,
}

impl ActionChain {
    pub fn single(a: impl Action + 'static) -> Self {
        Self {
            actions: vec![Arc::new(a)],
        }
    }

    pub fn then(mut self, a: impl Action + 'static) -> Self {
        self.actions.push(Arc::new(a));
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Action>> {
        self.actions.iter()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl From<Arc<dyn Action>> for ActionChain {
    fn from(a: Arc<dyn Action>) -> Self {
        Self { actions: vec![a] }
    }
}

pub struct TabDispatcher {
    pub locked: bool,
}

impl TabDispatcher {
    fn new() -> Self {
        Self { locked: false }
    }
}

pub struct Dispatcher {
    pub event_tx: broadcast::Sender<PipelineEvent>,
    pub tabs: HashMap<SessionId, TabDispatcher>,
    active_apply_actions: HashMap<SessionId, Arc<dyn Action>>,
    background_actions: HashMap<SessionId, Arc<dyn Action>>,
    background_tasks: HashMap<SessionId, PipelineHandle>,
}

impl Dispatcher {
    pub fn new(event_tx: broadcast::Sender<PipelineEvent>) -> Self {
        Self {
            event_tx,
            tabs: HashMap::new(),
            active_apply_actions: HashMap::new(),
            background_actions: HashMap::new(),
            background_tasks: HashMap::new(),
        }
    }

    fn tab_disp(&mut self, id: SessionId) -> &mut TabDispatcher {
        self.tabs.entry(id).or_insert_with(TabDispatcher::new)
    }

    pub fn dispatch(
        &mut self,
        action: Arc<dyn Action>,
        state: &mut EditorState,
    ) -> Result<(), String> {
        let session_id = action.target_tab();

        if let Some(tid) = session_id {
            let td = self.tab_disp(tid);
            if td.locked {
                return Err("Pipeline running on tab, please wait".to_string());
            }
        }

        match action.prepare(state)? {
            PreparedAction::StateOnly => {
                action.apply(state, PipelineStatus::Done);
                if action.record_in_history() {
                    // TODO: push HistoryEntry
                }
                Ok(())
            }
            PreparedAction::Pipeline {
                mode,
                graph,
                routed_tab,
                ..
            } => {
                let is_apply = mode == PipelineMode::Apply;
                let effective_session = routed_tab.or(session_id);
                let tag = effective_session.map(|t| t.0).unwrap_or(0);

                if is_apply && let Some(tid) = effective_session {
                    self.tab_disp(tid).locked = true;
                    self.active_apply_actions.insert(tid, Arc::clone(&action));
                }

                let cancelled = Arc::new(AtomicBool::new(false));
                let (event_tx, event_rx) = sync_channel::<PipelineEvent>(64);
                let pipeline =
                    Pipeline::compile(graph, Some(event_tx.clone()), cancelled.clone(), tag)
                        .map_err(|e| e.to_string())?;

                let broadcast_tx = self.event_tx.clone();
                thread::spawn(move || {
                    while let Ok(event) = event_rx.recv() {
                        let tagged = match event {
                            PipelineEvent::Error { message, .. } => {
                                PipelineEvent::Error { tag, message }
                            }
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

                if !is_apply && let Some(tid) = effective_session {
                    self.background_tasks.insert(tid, handle);
                    self.background_actions.insert(tid, Arc::clone(&action));
                }

                Ok(())
            }
        }
    }

    pub fn on_pipeline_done(&mut self, state: &mut EditorState, session_id: SessionId) {
        if let Some(action) = self
            .active_apply_actions
            .remove(&session_id)
            .or_else(|| self.background_actions.remove(&session_id))
        {
            action.apply(state, PipelineStatus::Done);
        }
        if let Some(td) = self.tabs.get_mut(&session_id) {
            td.locked = false;
        }
    }

    pub fn on_pipeline_error(
        &mut self,
        state: &mut EditorState,
        session_id: SessionId,
        error: String,
    ) {
        if let Some(action) = self
            .active_apply_actions
            .remove(&session_id)
            .or_else(|| self.background_actions.remove(&session_id))
        {
            action.apply(state, PipelineStatus::Error(error));
        }
        if let Some(td) = self.tabs.get_mut(&session_id) {
            td.locked = false;
        }
    }

    #[allow(dead_code)]
    pub fn cleanup_tab(&mut self, id: SessionId) {
        self.tabs.remove(&id);
        self.background_tasks.remove(&id);
        self.background_actions.remove(&id);
        self.active_apply_actions.remove(&id);
    }

    pub fn cancel_background(&mut self, id: SessionId) {
        if let Some(handle) = self.background_tasks.remove(&id) {
            handle.cancel();
        }
    }

    /// Run a pre-built graph without a state-mutating Action. Used by the
    /// desktop layer for viewport-only pipelines (MipFetch, BlurPreview, etc.)
    /// that have no state side-effects.
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

    pub fn resync_locks(&mut self, state: &mut EditorState) {
        self.background_tasks.retain(|session_id, handle| {
            let still_running = handle.is_running();
            if !still_running && let Some(tab) = state.session_mut(*session_id) {
                tab.transient.view.loading = false;
                tab.transient.view.progress = 1.0;
            }
            still_running
        });
        self.background_actions
            .retain(|session_id, _| self.background_tasks.contains_key(session_id));
        for tab in &mut self.tabs.values_mut() {
            tab.locked = false;
        }
    }
}
