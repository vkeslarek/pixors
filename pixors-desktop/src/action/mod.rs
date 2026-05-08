pub mod actions;

use std::collections::HashMap;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::thread;

use pixors_executor::graph::graph::ExecGraph;
use pixors_executor::runtime::event::PipelineEvent;
use pixors_executor::runtime::pipeline::Pipeline;
use tokio::sync::broadcast;

use crate::state::history::SnapshotId;
use crate::state::{EditorState, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Background,
    Apply,
}

#[derive(Debug, Clone)]
pub enum PipelineStatus {
    Done,
    Error(String),
    Cancelled,
}

pub enum PreparedAction {
    StateOnly,
    Pipeline {
        mode: PipelineMode,
        graph: ExecGraph,
        snapshot: Option<SnapshotId>,
    },
}

pub trait Action: std::fmt::Debug + Send + Sync + 'static {
    fn target_tab(&self) -> Option<TabId> {
        None
    }

    fn prepare(&self, state: &mut EditorState) -> Result<PreparedAction, String>;

    fn apply(&self, state: &mut EditorState, status: PipelineStatus);

    fn undo(&self, state: &mut EditorState);

    fn record_in_history(&self) -> bool {
        true
    }
}

pub struct TabDispatcher {
    pub pending_action: Option<Arc<dyn Action>>,
    pub locked: bool,
}

impl TabDispatcher {
    fn new() -> Self {
        Self {
            pending_action: None,
            locked: false,
        }
    }
}

pub struct Dispatcher {
    pub event_tx: broadcast::Sender<PipelineEvent>,
    pub tabs: HashMap<TabId, TabDispatcher>,
    active_pipeline_tab: Option<TabId>,
}

impl Dispatcher {
    pub fn new(event_tx: broadcast::Sender<PipelineEvent>) -> Self {
        Self {
            event_tx,
            tabs: HashMap::new(),
            active_pipeline_tab: None,
        }
    }

    fn tab_disp(&mut self, id: TabId) -> &mut TabDispatcher {
        self.tabs.entry(id).or_insert_with(TabDispatcher::new)
    }

    pub fn dispatch(
        &mut self,
        action: Arc<dyn Action>,
        state: &mut EditorState,
    ) -> Result<(), String> {
        let tab_id = action.target_tab();

        if let Some(tid) = tab_id {
            let td = self.tab_disp(tid);
            if td.locked {
                return Err(format!(
                    "Pipeline running on tab, please wait"
                ));
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
            PreparedAction::Pipeline { mode, graph, .. } => {
                if mode == PipelineMode::Apply {
                    if let Some(tid) = tab_id {
                        self.tab_disp(tid).locked = true;
                    }
                }

                if let Some(tid) = tab_id {
                    self.tab_disp(tid).pending_action = Some(Arc::clone(&action));
                }
                self.active_pipeline_tab = tab_id;

                let (event_tx, event_rx) = sync_channel::<PipelineEvent>(64);
                let pipeline = Pipeline::compile(&graph, Some(event_tx.clone()))
                    .map_err(|e| e.to_string())?;

                let broadcast_tx = self.event_tx.clone();
                thread::spawn(move || {
                    if let Err(e) = pipeline.run(None) {
                        tracing::error!("[pixors] pipeline error: {e}");
                    }
                    let _ = event_tx.send(PipelineEvent::Done);
                });
                thread::spawn(move || {
                    while let Ok(event) = event_rx.recv() {
                        let _ = broadcast_tx.send(event);
                    }
                });

                Ok(())
            }
        }
    }

    pub fn on_pipeline_done(&mut self, state: &mut EditorState) {
        if let Some(tid) = self.active_pipeline_tab.take() {
            if let Some(action) = self.tab_disp(tid).pending_action.take() {
                action.apply(state, PipelineStatus::Done);
            }
            self.tab_disp(tid).locked = false;
        }
    }

    pub fn on_pipeline_error(&mut self, state: &mut EditorState, error: String) {
        if let Some(tid) = self.active_pipeline_tab.take() {
            if let Some(action) = self.tab_disp(tid).pending_action.take() {
                action.apply(state, PipelineStatus::Error(error));
            }
            self.tab_disp(tid).locked = false;
        }
    }

    pub fn cleanup_tab(&mut self, id: TabId) {
        self.tabs.remove(&id);
    }
}
