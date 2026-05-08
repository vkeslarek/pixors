use std::collections::HashMap;
use std::path::PathBuf;

use super::tab::{BlendMode, LayerId, LayerSource};

#[derive(Default)]
pub struct History {
    pub past: Vec<HistoryEntry>,
    pub future: Vec<HistoryEntry>,
    pub cache: HistoryCache,
}

pub struct HistoryEntry {
    pub action_label: String,
    pub snapshot_id: SnapshotId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotId(pub u64);

#[derive(Default)]
pub struct HistoryCache {
    pub snapshots: HashMap<SnapshotId, Snapshot>,
    pub next_id: u64,
}

pub struct Snapshot {
    pub layer_states: Vec<LayerSnapshot>,
    pub tile_archive: PathBuf,
}

pub struct LayerSnapshot {
    pub layer: LayerId,
    pub source: LayerSource,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
}


