use crate::document::Document;

/// A reversible, serializable operation on a Document.
#[typetag::serde(tag = "type")]
pub trait DocumentMutation: std::fmt::Debug + Send + Sync {
    fn apply(&self, doc: &mut Document);
    fn undo(&self, doc: &mut Document);
    fn label(&self) -> &str;
}

pub mod impls;
