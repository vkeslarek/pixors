use std::sync::Arc;

use pixors_document::mutation::Mutation;
use pixors_document::{NodeId, SessionId};
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
    /// Select a layer in the layers panel (UI state, not a document mutation).
    SelectLayer {
        session_id: SessionId,
        layer_id: NodeId,
    },
    /// Push an error toast.
    PushError(String),
    /// No effect.
    None,
}
