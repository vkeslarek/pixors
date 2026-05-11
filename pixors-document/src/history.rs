use std::collections::HashMap;
use std::path::PathBuf;

use pixors_image::image::BlendMode;

use crate::document::{NodeId, PixelSource};

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
    pub layer: NodeId,
    pub source: PixelSource,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
}
