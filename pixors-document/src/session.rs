use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixors_engine::cache::disk_cache::DiskCache;
use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::PixelFormat;

use crate::document::Document;
use crate::document::NodeId;
use crate::history::History;
use crate::tab::SessionId;

#[derive(Debug, Clone, Default)]
pub struct ViewState {
    pub active_mip: u32,
    pub loading: bool,
    pub progress: f32,
}

/// Transient editing state for one session. Never serialized to disk.
pub struct Transient {
    pub cache_dir: PathBuf,
    pub redraw_seq: u64,
    pub view: ViewState,
    pub active_node: Option<NodeId>,
    pub pipeline_running: bool,
    pub progress: f32,
    pub disk_caches: HashMap<NodeId, Arc<DiskCache>>,
}

impl std::fmt::Debug for Transient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transient")
            .field("cache_dir", &self.cache_dir)
            .field("redraw_seq", &self.redraw_seq)
            .field("view", &self.view)
            .field("active_node", &self.active_node)
            .field("pipeline_running", &self.pipeline_running)
            .field("progress", &self.progress)
            .field("disk_caches", &self.disk_caches.len())
            .finish()
    }
}

impl Transient {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            redraw_seq: 0,
            view: ViewState::default(),
            active_node: None,
            pipeline_running: false,
            progress: 0.0,
            disk_caches: HashMap::new(),
        }
    }

    pub fn get_or_create_disk_cache(&mut self, layer_id: NodeId) -> Arc<DiskCache> {
        self.disk_caches
            .entry(layer_id)
            .or_insert_with(|| {
                let dir = crate::document::cache::layer_cache_dir(&self.cache_dir, layer_id);
                Arc::new(DiskCache::new(dir, 64 * 1024 * 1024))
            })
            .clone()
    }

    pub fn cleanup_disk_caches(&self) {
        for cache in self.disk_caches.values() {
            cache.cleanup();
        }
    }
}

/// A single editing session — the unit of work.
/// Owns a document, its undo history, and transient runtime state.
/// Desktop wraps this in a Tab; MCP uses it headlessly.
#[derive(Debug)]
pub struct Session {
    pub id: SessionId,
    pub document: Document,
    pub history: History,
    pub transient: Transient,
    pub working_format: PixelFormat,
    pub working_color_space: ColorSpace,
    pub display_format: PixelFormat,
    pub display_color_space: ColorSpace,
}

impl Session {
    pub fn layer_cache_dir(&self, node_id: NodeId) -> PathBuf {
        crate::document::cache::layer_cache_dir(&self.transient.cache_dir, node_id)
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.document.assets.primary_path.as_deref()
    }
}
