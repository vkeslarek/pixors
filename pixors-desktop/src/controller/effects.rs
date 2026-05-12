
use crate::app::App;
use crate::effect::Effect;
use pixors_document::TILE_SIZE;

impl App {
    pub(crate) fn execute_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Commit(mutation) => {
                    let session_id = mutation.target_session();
                    let needs_recompile = mutation.needs_recompile();
                    if let Err(e) = self.dispatcher.commit(mutation, &mut self.state) {
                        self.push_error(e);
                    }
                    if needs_recompile {
                        self.recomposite_current_view(session_id);
                    }
                }
                Effect::Preview(mutation) => {
                    let session_id = mutation.target_session();
                    let Some(op) = self.dispatcher.preview_op(mutation.as_ref()) else {
                        continue;
                    };
                    self.dispatcher.cancel_background(session_id);
                    if let Some(tab) = self.state.session_mut(session_id) {
                        tab.transient.redraw_seq = tab.transient.redraw_seq.wrapping_add(1);
                    }
                    let generation = self
                        .state
                        .session(session_id)
                        .map(|t| t.transient.redraw_seq)
                        .unwrap_or(0);
                    if let Some(cache) = self.viewport_tabs.get(&session_id).map(|vt| &vt.cache)
                        && let Ok(mut guard) = cache.lock()
                    {
                        guard.active_generation = generation;
                    }
                    let (mip, range) = self
                        .viewport_tabs
                        .get(&session_id)
                        .and_then(|vt| vt.state.read().ok())
                        .map(|vs| {
                            let m = vs.current_mip;
                            let r = vs.camera.padded_tile_range(m, TILE_SIZE, 3);
                            (m, r)
                        })
                        .unwrap_or((
                            0,
                            pixors_engine::cache::cache_reader::TileRange {
                                tx_start: 0,
                                tx_end: 0,
                                ty_start: 0,
                                ty_end: 0,
                            },
                        ));
                    self.run_blur_preview_generic(session_id, &op, generation, mip, range);
                }
                Effect::RunGraph { graph, session_id } => {
                    let _ = self.dispatcher.run_graph(graph, session_id);
                }
                Effect::QueueDisplayRefresh(session_id) => {
                    self.recomposite_current_view(session_id);
                }
                Effect::CancelBackground(session_id) => {
                    self.dispatcher.cancel_background(session_id);
                }
                Effect::ClearOverlay(session_id) => {
                    if let Some(cache) = self.viewport_tabs.get(&session_id).map(|vt| &vt.cache)
                        && let Ok(mut guard) = cache.lock()
                    {
                        let generation = self
                            .state
                            .session(session_id)
                            .map(|t| t.transient.redraw_seq)
                            .unwrap_or(0);
                        guard.clear_generation(generation);
                    }
                }
                Effect::ShowFilterSearch => self.show_filter_search = true,
                Effect::TogglePane(kind) => self.toggle_pane(kind),
                Effect::SelectLayer {
                    session_id,
                    layer_id,
                } => {
                    if let Some(tab) = self.state.session_mut(session_id) {
                        tab.transient.active_node = Some(layer_id);
                    }
                }
                Effect::PushError(msg) => self.push_error(msg),
                Effect::None => {}
            }
        }
    }
}
