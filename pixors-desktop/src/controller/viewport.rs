use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pixors_document::render::compiler::{CompileConfig, RenderRequest, compile};
use pixors_document::{SessionId, TILE_SIZE};
use pixors_engine::cache::cache_reader::TileRange;
use pixors_engine::data::tile::TileGridPos;
use pixors_engine::stage::Stage;

use crate::app::App;
use crate::viewport::tile_cache::CachedTile;
use crate::viewport::tile_cache_sink::TileCacheSink;

impl App {
    fn compile_config(&self, session_id: SessionId) -> Option<CompileConfig> {
        let tab = self.state.session(session_id)?;
        Some(CompileConfig {
            disk_caches: tab.transient.disk_caches.clone(),
            cache_dir: tab.transient.cache_dir.clone(),
            display_format: tab.display_format,
            display_color_space: tab.display_color_space,
            working_format: tab.working_format,
            working_color_space: tab.working_color_space,
            tile_size: TILE_SIZE,
            img_w: tab.document.canvas.width,
            img_h: tab.document.canvas.height,
        })
    }

    pub(crate) fn viewport_mip_range(&self, session_id: SessionId, pad: u32) -> (u32, TileRange) {
        self.viewport_tabs
            .get(&session_id)
            .and_then(|vt| vt.state.read().ok())
            .map(|vs| {
                // Use the ideal mip from camera geometry, not current_mip which the
                // display path may degrade to a lower mip while tiles are still loading.
                // Guard: if the viewport hasn't been sized yet (vp_w/h == 1.0),
                // visible_mip_level() returns 0 and current_mip is equally wrong — fall
                // back to current_mip which is at least consistent with whatever was last
                // computed in draw().
                let m = if vs.camera.vp_w > 1.0 && vs.camera.vp_h > 1.0 {
                    vs.camera.visible_mip_level()
                } else {
                    vs.current_mip
                };
                let r = vs.camera.padded_tile_range(m, TILE_SIZE, pad);
                (m, r)
            })
            .unwrap_or((
                0,
                TileRange {
                    tx_start: 0,
                    tx_end: 0,
                    ty_start: 0,
                    ty_end: 0,
                },
            ))
    }

    pub(crate) fn run_render(&mut self, session_id: SessionId, mip: u32, range: TileRange) {
        self.dispatcher.cancel_background(session_id);
        let Some(tab) = self.state.session(session_id) else {
            return;
        };

        let visible: Vec<&pixors_document::LayerNode> = tab
            .document
            .visible_layers()
            .into_iter()
            .filter(|l| tab.layer_cache_dir(l.id).exists())
            .collect();
        if visible.is_empty() {
            let version = tab.transient.redraw_seq;
            let cw = tab.document.canvas.width;
            let ch = tab.document.canvas.height;
            let scale = 1u32 << mip;
            let img_w = cw.div_ceil(scale);
            let img_h = ch.div_ceil(scale);
            if let Some(cache) = self.viewport_tabs.get(&session_id).map(|vt| &vt.cache)
                && let Ok(mut guard) = cache.lock()
            {
                for ty in range.ty_start..range.ty_end {
                    for tx in range.tx_start..range.tx_end {
                        let px = tx * TILE_SIZE;
                        let py = ty * TILE_SIZE;
                        if px >= img_w || py >= img_h {
                            continue;
                        }
                        let tw = (img_w - px).min(TILE_SIZE);
                        let th = (img_h - py).min(TILE_SIZE);
                        guard.insert(
                            0,
                            version,
                            TileGridPos {
                                mip_level: mip,
                                tx,
                                ty,
                            },
                            CachedTile {
                                px,
                                py,
                                width: tw,
                                height: th,
                                bpp: 4,
                                bytes: Arc::new(vec![0u8; (tw * th * 4) as usize]),
                                layer: 0,
                            },
                        );
                    }
                }
            }
            return;
        }

        let config = self.compile_config(session_id).unwrap();
        let req = RenderRequest {
            viewport: range,
            mip_level: mip,
            up_to: None,
        };
        let version = tab.transient.redraw_seq;
        let sink = Stage::Consumer(Box::new(self.make_tile_cache_sink(session_id, 0, version)));
        let graph = compile(&tab.document, &req, &config, sink);

        let _ = self.dispatcher.run_graph(graph, Some(session_id));
    }

    /// Warm the DiskCache LRU for the predicted region without touching the visible pipeline.
    /// Spawns a background thread that calls `DiskCache::read_tile` for each tile in `range`;
    /// the side-effect populates the LRU so subsequent visible renders get cache hits.
    pub(crate) fn run_prefetch(&mut self, session_id: SessionId, mip: u32, range: TileRange) {
        let Some(tab) = self.state.session(session_id) else {
            return;
        };
        let Some(vtab) = self.viewport_tabs.get_mut(&session_id) else {
            return;
        };

        // Cancel the previous prefetch thread for this session.
        vtab.prefetch_cancel.store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        vtab.prefetch_cancel = cancel.clone();

        let caches: Vec<_> = tab
            .document
            .visible_layers()
            .into_iter()
            .filter_map(|l| tab.transient.disk_caches.get(&l.id).cloned())
            .collect();

        if caches.is_empty() {
            return;
        }

        std::thread::spawn(move || {
            for ty in range.ty_start..range.ty_end {
                for tx in range.tx_start..range.tx_end {
                    if cancel.load(Ordering::Acquire) {
                        return;
                    }
                    for cache in &caches {
                        let _ = cache.read_tile(mip, tx, ty);
                    }
                }
            }
        });
    }

    pub(crate) fn init_viewport_for_tab(&mut self, session_id: SessionId) {
        let Some(tab) = self.state.session(session_id) else {
            return;
        };
        let img_w = tab.document.canvas.width;
        let img_h = tab.document.canvas.height;

        let vtab = crate::viewport::tab_state::ViewportTab::new();
        vtab.init_for_image(img_w, img_h);

        self.viewport_tabs.insert(session_id, vtab);

        let ntx = img_w.div_ceil(TILE_SIZE);
        let nty = img_h.div_ceil(TILE_SIZE);
        let full_range = TileRange {
            tx_start: 0,
            tx_end: ntx,
            ty_start: 0,
            ty_end: nty,
        };
        self.run_render(session_id, 0, full_range);
    }

    pub(crate) fn make_tile_cache_sink(
        &self,
        session_id: SessionId,
        generation: u64,
        version: u64,
    ) -> TileCacheSink {
        let cache = self
            .viewport_tabs
            .get(&session_id)
            .map(|vt| vt.cache.clone())
            .expect("viewport tab not found");
        TileCacheSink::new(
            generation,
            version,
            Arc::new(
                move |generation, version, mip, tx, ty, px, py, tw, th, bpp, bytes| {
                    if let Ok(mut guard) = cache.lock() {
                        guard.insert(
                            generation,
                            version,
                            TileGridPos {
                                mip_level: mip,
                                tx,
                                ty,
                            },
                            CachedTile {
                                px,
                                py,
                                width: tw,
                                height: th,
                                bpp,
                                bytes: Arc::new(bytes.to_vec()),
                                layer: generation,
                            },
                        );
                    }
                },
            ),
        )
    }
}
