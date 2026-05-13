use crate::app::App;
use crate::effect::Effect;

impl App {
    pub(crate) fn execute_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Commit(mutation) => {
                    let session_id = mutation.target_session();
                    if let Err(e) = self.dispatcher.commit(mutation, &mut self.state) {
                        self.push_error(e);
                    }
                    self.recomposite_current_view(session_id);
                }
                Effect::Preview(_mutation) => {
                    // Handled directly by controller methods (handle_layers_msg, handle_filters_msg)
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
                Effect::ClearOverlay(_session_id) => {
                    // Overlay cache removed; no-op.
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
