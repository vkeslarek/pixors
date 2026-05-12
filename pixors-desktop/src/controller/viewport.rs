use std::sync::Arc;

use pixors_document::action::PipelineMode;
use pixors_document::render::compiler::{CompileConfig, RenderRequest, compile};
use pixors_document::{TILE_SIZE, TabId};
use pixors_engine::data::tile::TileGridPos;
use pixors_engine::stage::Stage;
use pixors_ops::source::cache_reader::TileRange;

use crate::app::App;
use crate::viewport::tile_cache::CachedTile;
use crate::viewport::tile_cache_sink::{TileCacheSink, register_tile_cache};

impl App {
    /// Create viewport state for a newly opened tab and trigger initial MipFetch.
    pub(crate) fn init_viewport_for_tab(&mut self, tab_id: TabId) {
        let Some(tab) = self.state.tab(tab_id) else {
            return;
        };
        let img_w = tab.document.canvas.width;
        let img_h = tab.document.canvas.height;

        let vtab = crate::viewport::tab_state::ViewportTab::new();
        vtab.init_for_image(img_w, img_h);

        // Register sink callback (pipeline → RAM cache).
        {
            let cache = vtab.cache.clone();
            register_tile_cache(
                tab_id.0,
                Box::new(
                    move |generation, version, mip, tx, ty, px, py, tw, th, bytes| {
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
                                    bytes: Arc::new(bytes.to_vec()),
                                    layer: generation,
                                },
                            );
                        }
                    },
                ),
            );
        }

        self.viewport_tabs.insert(tab_id, vtab);

        // Trigger full mip-0 fetch so tiles appear immediately.
        let ntx = img_w.div_ceil(TILE_SIZE);
        let nty = img_h.div_ceil(TILE_SIZE);
        let full_range = TileRange {
            tx_start: 0,
            tx_end: ntx,
            ty_start: 0,
            ty_end: nty,
        };
        self.run_mip_fetch(tab_id, 0, full_range);
    }

    pub(crate) fn run_mip_fetch(&mut self, tab_id: TabId, mip: u32, range: TileRange) {
        let Some(tab) = self.state.tab(tab_id) else {
            return;
        };

        let visible: Vec<&pixors_document::LayerNode> = tab
            .document
            .visible_layers()
            .into_iter()
            .filter(|l| tab.layer_cache_dir(l.id).exists())
            .collect();
        if visible.is_empty() {
            // Write transparent tiles so the viewport clears instead of showing stale data.
            let cw = tab.document.canvas.width;
            let ch = tab.document.canvas.height;
            let scale = 1u32 << mip;
            let img_w = cw.div_ceil(scale);
            let img_h = ch.div_ceil(scale);
            if let Some(cache) = self.viewport_tabs.get(&tab_id).map(|vt| &vt.cache)
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
                            u64::MAX,
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
                                bytes: Arc::new(vec![0u8; (tw * th * 4) as usize]),
                                layer: 0,
                            },
                        );
                    }
                }
            }
            return;
        }

        let config = CompileConfig {
            cache_dir: tab.session.cache_dir.clone(),
            display_format: self.state.display_format,
            display_color_space: self.state.display_color_space,
            working_format: self.state.working_format,
            working_color_space: self.state.working_color_space,
            tile_size: TILE_SIZE,
            img_w: tab.document.canvas.width,
            img_h: tab.document.canvas.height,
        };
        let req = RenderRequest {
            viewport: range,
            mip_level: mip,
            up_to: None,
        };
        let version = tab.session.redraw_seq;
        let sink = Stage::Consumer(Box::new(TileCacheSink::new(tab_id.0, 0, version)));
        let graph = compile(&tab.document, &req, &config, sink);

        let _ = self
            .dispatcher
            .run_graph(graph, PipelineMode::Background, Some(tab_id));
    }
}
