use crate::util::{lock_or_recover, read_or_recover, write_or_recover};
use std::sync::{Arc, Mutex, RwLock};

use pixors_document::SessionId;
use pixors_engine::cache::cache_reader::TileRange;

use crate::viewport::tile_cache::TileCache;
use crate::viewport::viewport_state::ViewportState;

pub struct ViewportTab {
    pub cache: Arc<Mutex<TileCache>>,
    pub state: Arc<RwLock<ViewportState>>,
    pub mip_queue: Arc<Mutex<Vec<(SessionId, u32, TileRange)>>>,
}

impl ViewportTab {
    pub fn new() -> Self {
        Self {
            cache: TileCache::new(),
            state: Arc::new(RwLock::new(ViewportState::default())),
            mip_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn init_for_image(&self, img_w: u32, img_h: u32) {
        lock_or_recover(&self.cache).signal_new_img(img_w, img_h);
        let mut vs = write_or_recover(&self.state);
        vs.camera.img_w = img_w as f32;
        vs.camera.img_h = img_h as f32;
    }
}
