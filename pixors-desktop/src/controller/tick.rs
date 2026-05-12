use pixors_document::SessionId;
use pixors_engine::cache::cache_reader::TileRange;

use crate::app::App;

impl App {
    pub(crate) fn handle_tick(&mut self) {
        self.errors.retain(|(_, ts)| ts.elapsed().as_secs() < 5);

        // Detect new images and fit camera (moved from ViewportProgram::draw)
        for tab in &mut self.state.sessions {
            if let Some(vtab) = self.viewport_tabs.get(&tab.id)
                && let Ok(mut guard) = vtab.cache.lock()
                && let Some((img_w, img_h)) = guard.take_new_img()
            {
                let mut vs = vtab.state.write().unwrap();
                vs.camera.img_w = img_w as f32;
                vs.camera.img_h = img_h as f32;
                vs.camera.fit();
                vs.current_mip = vs.camera.visible_mip_level();
            }
        }

        let mut mip_requests: Vec<(SessionId, u32, TileRange)> = Vec::new();

        for tab in &mut self.state.sessions {
            if let Some(cache) = self.viewport_tabs.get(&tab.id).map(|vt| &vt.cache)
                && cache.lock().is_ok_and(|g| g.has_pending())
            {
                tab.transient.redraw_seq = tab.transient.redraw_seq.wrapping_add(1);
            }

            if let Some(queue) = self.viewport_tabs.get(&tab.id).map(|vt| &vt.mip_queue) {
                let mut sigs = queue.lock().unwrap();
                if !sigs.is_empty() {
                    for (session_id, mip, range) in sigs.drain(..) {
                        mip_requests.push((session_id, mip, range));
                    }
                }
            }
        }

        for (session_id, mip, range) in mip_requests {
            if let Some(cache) = self.viewport_tabs.get(&session_id).map(|vt| &vt.cache)
                && let Ok(guard) = cache.lock()
                && guard.has_all_tiles(mip, &range)
            {
                continue;
            }
            self.run_mip_fetch(session_id, mip, range);
        }
    }
}
