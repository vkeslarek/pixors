use std::sync::{Arc, Mutex, RwLock};

use pixors_document::SessionId;
use pixors_ops::source::cache_reader::TileRange;

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
        self.cache.lock().unwrap().signal_new_img(img_w, img_h);
        let mut vs = self.state.write().unwrap();
        vs.camera.img_w = img_w as f32;
        vs.camera.img_h = img_h as f32;
    }
}
