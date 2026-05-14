use crate::util::{lock_or_recover, write_or_recover};
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
                let mut vs = write_or_recover(&vtab.state);
                vs.camera.img_w = img_w as f32;
                vs.camera.img_h = img_h as f32;
                vs.camera.fit();
                vs.zoom_target = vs.camera.zoom;
                vs.current_mip = vs.camera.visible_mip_level();
            }
        }

        let mut mip_requests: Vec<(SessionId, u32, TileRange)> = Vec::new();
        let mut prefetch_requests: Vec<(SessionId, u32, TileRange)> = Vec::new();

        for tab in &mut self.state.sessions {
            if let Some(vtab) = self.viewport_tabs.get(&tab.id) {
                if let Ok(cache) = vtab.cache.lock() {
                    let pending = cache.has_pending();
                    if pending {
                        tab.transient.redraw_seq = tab.transient.redraw_seq.wrapping_add(1);
                    } else if tab.transient.view.loading
                        && !self.dispatcher.is_background_running(tab.id)
                    {
                        // Spinner was kept alive while tiles drained; hide it now.
                        tab.transient.view.loading = false;
                    }
                }

                let mut sigs = lock_or_recover(&vtab.mip_queue);
                if !sigs.is_empty() {
                    for (session_id, mip, range) in sigs.drain(..) {
                        mip_requests.push((session_id, mip, range));
                    }
                }

                let mut pf = lock_or_recover(&vtab.prefetch_queue);
                if !pf.is_empty() {
                    for (session_id, mip, range) in pf.drain(..) {
                        prefetch_requests.push((session_id, mip, range));
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
            // Don't cancel a running render — it would reset indefinitely on large images.
            // Wait for it to finish; the viewport will start the correct-mip render after Done.
            if self.dispatcher.is_background_running(session_id) {
                continue;
            }
            self.run_render(session_id, mip, range);
        }

        // Prefetch after visible renders so visible work always runs first.
        for (session_id, mip, range) in prefetch_requests {
            self.run_prefetch(session_id, mip, range);
        }
    }
}
