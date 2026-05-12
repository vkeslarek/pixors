use std::path::{Path, PathBuf};

use super::NodeId;

pub fn layer_cache_dir(root: &Path, layer: NodeId) -> PathBuf {
    root.join(format!("layer_{:016x}", layer.0))
}
