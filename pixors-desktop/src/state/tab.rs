use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pixors_executor::common::image::ImageDescriptor;
use pixors_executor::source::cache_reader::TileRange;

use crate::viewport::state::ViewportState;
use crate::viewport::tile_cache::ViewportCache;

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
    pub viewport_cache: Arc<Mutex<ViewportCache>>,
    pub viewport_state: Rc<RefCell<ViewportState>>,
    pub mip_fetch_signal: Arc<Mutex<Vec<(TabId, u32, TileRange)>>>,
    pub tile_generation: u64,
    pub layers: Vec<Layer>,
    pub active_layer: Option<LayerId>,
    pub chain: EditChain,
    pub history: History,
    pub view: TabView,
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
    pub zoom: f32,
    pub pan: (f32, f32),
    pub active_mip: u32,
    pub loading: bool,
    pub progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
}

pub struct EditChain {
    pub ops: Vec<()>,
}

impl Default for EditChain {
    fn default() -> Self {
        Self { ops: Vec::new() }
    }
}
