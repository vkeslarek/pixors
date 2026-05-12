use std::fmt::Debug;

use crate::document::Document;
use crate::document::Operation;
use crate::tab::SessionId;

/// Canonical unit of change — the only way to mutate document state.
///
/// Encapsulates:
/// 1. How it affects the Document (apply/undo)
/// 2. How it generates a preview pipeline (preview_op)
/// 3. Whether it requires a full recomposite (needs_recompile)
///
/// The Dispatcher orchestrates:
///   preview → compile_preview(preview_op) → overlay tiles
///   commit  → apply(document) + compile() if needs_recompile
///   undo    → undo(document) + compile()
#[typetag::serde(tag = "type")]
pub trait Mutation: Debug + Send + Sync {
    fn target_session(&self) -> SessionId;
    fn label(&self) -> &str;
    fn apply(&self, doc: &mut Document);
    fn undo(&self, doc: &mut Document);

    /// Returns an Operation for live preview. Called on every slider drag tick.
    /// None = no preview (e.g., visibility toggle).
    fn preview_op(&self) -> Option<Operation> {
        None
    }

    /// true = commit/undo requires a full recompile (compose layers again).
    /// false = only preview pipeline is affected (e.g., blur radius change).
    fn needs_recompile(&self) -> bool {
        false
    }

    /// true = should be recorded in History for undo/redo.
    fn recordable(&self) -> bool {
        true
    }
}

pub mod impls;
