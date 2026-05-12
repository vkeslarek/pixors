use std::sync::Arc;

use crate::app::App;
use crate::effect::Effect;

impl App {
    pub(crate) fn execute_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Dispatch(action) => {
                    if let Err(e) = self.dispatcher.dispatch(action, &mut self.state) {
                        self.push_error(e);
                    }
                }
                Effect::RunGraph {
                    graph,
                    mode,
                    session_id,
                } => {
                    let _ = self.dispatcher.run_graph(graph, mode, session_id);
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
                Effect::ShowFilterSearch => {
                    self.show_filter_search = true;
                }
                Effect::TogglePane(kind) => self.toggle_pane(kind),
                Effect::ToggleTransformEnabled {
                    session_id,
                    layer_id,
                    transform_id,
                    enabled,
                } => {
                    if let Some(tab) = self.state.session(session_id)
                        && let Some(layer) = tab.document.find_layer(layer_id)
                        && let Some(t) = layer.transforms.iter().find(|t| t.id == transform_id)
                    {
                        let _ = self.dispatcher.dispatch(
                            Arc::new(pixors_document::mutation::impls::SetTransformEnabled {
                                tab: session_id,
                                layer: layer_id,
                                transform_id: t.id,
                                before: t.enabled,
                                after: enabled,
                            }),
                            &mut self.state,
                        );
                    }
                }
                Effect::ReorderTransforms {
                    session_id,
                    layer_id,
                    from,
                    to,
                } => {
                    if let Some(tab) = self.state.session(session_id)
                        && let Some(_layer) = tab.document.find_layer(layer_id)
                        && from < _layer.transforms.len()
                        && to < _layer.transforms.len()
                    {
                        let _ = self.dispatcher.dispatch(
                            Arc::new(pixors_document::mutation::impls::ReorderTransform {
                                tab: session_id,
                                layer: layer_id,
                                from,
                                to,
                            }),
                            &mut self.state,
                        );
                        self.recomposite_current_view(session_id);
                    }
                }
                Effect::PushError(msg) => self.push_error(msg),
                Effect::None => {}
            }
        }
    }
}
