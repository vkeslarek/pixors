use std::sync::Arc;

use pixors_document::SessionId;
use pixors_document::mutation::Mutation;
use pixors_engine::graph::graph::ExecGraph;

use crate::app::PaneKind;

/// UI intent that the controller executes.
pub enum Effect {
    /// Commit a mutation: apply to Document + recompile if needed.
    /// Recorded in history for undo/redo.
    Commit(Arc<dyn Mutation>),
    /// Preview a mutation: run compile_preview with its preview_op.
    /// Called repeatedly during slider drag.
    Preview(Arc<dyn Mutation>),
    /// Action dispatched directly (Export, OpenFile — I/O operations).
    DispatchAction(Arc<dyn pixors_document::action::Action>),
    /// Run a background pipeline graph.
    RunGraph {
        graph: ExecGraph,
        session_id: Option<SessionId>,
    },
    /// Cancel in-flight background and re-request display tiles.
    QueueDisplayRefresh(SessionId),
    /// Cancel a running background pipeline for a tab.
    CancelBackground(SessionId),
    /// Clear overlay generation tiles from the tile cache.
    ClearOverlay(SessionId),
    /// Open the filter search modal.
    ShowFilterSearch,
    /// Toggle a pane open/closed.
    TogglePane(PaneKind),
    /// Push an error toast.
    PushError(String),
    /// No effect.
    None,
}
