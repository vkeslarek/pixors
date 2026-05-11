use std::sync::Arc;

use pixors_document::action::PipelineMode;
use pixors_document::{NodeId, TabId};
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
        tab_id: Option<TabId>,
    },
    /// Cancel in-flight background pipeline and re-request display tiles.
    QueueDisplayRefresh(TabId),
    /// Cancel a running background pipeline for a tab.
    CancelBackground(TabId),
    /// Clear overlay generation tiles from the tile cache.
    ClearOverlay(TabId),
    /// Open the filter search modal.
    ShowFilterSearch,
    /// Toggle a pane open/closed.
    TogglePane(PaneKind),
    /// Toggle a transform's enabled state (dispatches UpdateTransformOp and mutates).
    ToggleTransformEnabled {
        tab_id: TabId,
        layer_id: NodeId,
        transform_id: NodeId,
        enabled: bool,
    },
    /// Reorder transforms within a layer (direct mutation, then redraw).
    ReorderTransforms {
        tab_id: TabId,
        layer_id: NodeId,
        from: usize,
        to: usize,
    },
    /// Push an error toast.
    PushError(String),
    /// No effect.
    None,
}
