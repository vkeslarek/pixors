use std::sync::Arc;

use pixors_document::action::PipelineMode;
use pixors_document::{NodeId, SessionId};
use pixors_engine::graph::graph::ExecGraph;

use crate::app::PaneKind;

/// Side-effects that panel `update()` functions return.
/// The controller executes them — panels never touch `App` or `Dispatcher`.
pub enum Effect {
    /// Dispatch an action through the dispatcher.
    Dispatch(Arc<dyn pixors_document::action::Action>),
    /// Run a pipeline graph (background or foreground).
    RunGraph {
        graph: ExecGraph,
        mode: PipelineMode,
        session_id: Option<SessionId>,
    },
    /// Cancel in-flight background pipeline and re-request display tiles.
    QueueDisplayRefresh(SessionId),
    /// Cancel a running background pipeline for a tab.
    CancelBackground(SessionId),
    /// Clear overlay generation tiles from the tile cache.
    ClearOverlay(SessionId),
    /// Open the filter search modal.
    ShowFilterSearch,
    /// Toggle a pane open/closed.
    TogglePane(PaneKind),
    /// Toggle a transform's enabled state (dispatches UpdateTransformOp and mutates).
    ToggleTransformEnabled {
        session_id: SessionId,
        layer_id: NodeId,
        transform_id: NodeId,
        enabled: bool,
    },
    /// Reorder transforms within a layer (direct mutation, then redraw).
    ReorderTransforms {
        session_id: SessionId,
        layer_id: NodeId,
        from: usize,
        to: usize,
    },
    /// Push an error toast.
    PushError(String),
    /// No effect.
    None,
}
