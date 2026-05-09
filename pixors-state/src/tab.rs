use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use pixors_image::image::ImageDescriptor;
use pixors_ops::source::cache_reader::TileRange;

use crate::viewport::state::ViewportState;
use crate::viewport::tile_cache::TileCache;

use super::history::History;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub source: TabSource,
    pub desc: ImageDescriptor,
    pub cache_dir: PathBuf,
    pub tile_cache: Arc<Mutex<TileCache>>,
    pub viewport_state: Arc<RwLock<ViewportState>>,
    pub mip_fetch_queue: Arc<Mutex<Vec<(TabId, u32, TileRange)>>>,
    pub redraw_seq: u64,
    pub layers: Vec<Layer>,
    pub active_layer: Option<LayerId>,
    pub history: History,
    pub view: TabView,
    pub filter: FilterState,
}

pub enum TabSource {
    File { path: PathBuf },
    NewBlank { width: u32, height: u32 },
}

pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    pub source: LayerSource,
}

pub enum LayerSource {
    FilePage { page: usize },
    SolidColor { color: [u8; 4] },
}

pub struct TabView {
    pub active_mip: u32,
    pub loading: bool,
    pub progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
}

#[derive(Debug, Clone)]
pub struct FilterState {
    pub blur_radius: f32,
}

impl Default for FilterState {
    fn default() -> Self {
        Self { blur_radius: 3.0 }
    }
}

#[derive(Default)]
pub struct EditChain {
    pub ops: Vec<()>,
}
