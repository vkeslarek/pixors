use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable identifier for an asset in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub u64);

/// Store of external assets referenced by the document.
/// Phase 10: only holds the primary (source) asset path.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AssetStore {
    pub primary_path: Option<PathBuf>,
}
