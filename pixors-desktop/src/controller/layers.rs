use pixors_document::{TILE_SIZE, TabId};
use pixors_ops::source::cache_reader::TileRange;

use crate::app::App;
use crate::panel::layers as layers_panel;

impl App {
    pub(crate) fn handle_layers_msg(&mut self, m: layers_panel::Msg) {
        let layers = self
            .state
            .active_tab()
            .map(|t| t.document.layers.as_slice())
            .unwrap_or(&[]);
        let ctx = layers_panel::LayersContext {
            active_tab_id: self.state.active_tab().map(|t| t.id),
            layers,
        };
        let effects = layers_panel::update(m, ctx);
        self.execute_effects(effects);
    }

    pub(crate) fn recomposite_current_view(&mut self, tab_id: TabId) {
        let (mip, range) = self
            .viewport_tabs
            .get(&tab_id)
            .and_then(|vt| vt.state.read().ok())
            .map(|vs| {
                let m = vs.current_mip;
                let r = vs.camera.padded_tile_range(m, TILE_SIZE, 3);
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
            ));
        self.run_mip_fetch(tab_id, mip, range);
    }
}
