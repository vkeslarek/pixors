use std::sync::Arc;

use pixors_document::SessionId;

use crate::app::App;
use crate::panel::layers as layers_panel;

impl App {
    pub(crate) fn handle_layers_msg(&mut self, m: layers_panel::Msg) {
        match &m {
            layers_panel::Msg::SetOpacityPreview(id, opacity) => {
                self.layers_panel.update(&m);
                let Some(session_id) = self.state.active_session().map(|t| t.id) else {
                    return;
                };
                let before = self
                    .state
                    .active_session()
                    .and_then(|t| t.document.find_layer(*id).map(|l| l.blend.opacity))
                    .unwrap_or(1.0);

                let mutation: Arc<dyn pixors_document::mutation::Mutation> = Arc::new(
                    pixors_document::mutation::impls::SetLayerOpacity {
                        tab: session_id,
                        layer: *id,
                        before,
                        after: *opacity,
                    },
                );

                if let Some(ref prev) = self.pending_preview {
                    prev.undo(&mut self.state.session_mut(session_id).unwrap().document);
                }
                mutation.apply(&mut self.state.session_mut(session_id).unwrap().document);
                self.pending_preview = Some(mutation.clone());
                let _ = self.dispatcher.preview(mutation, &mut self.state);

                let (mip, range) = self.viewport_mip_range(session_id, 3);
                self.run_render(session_id, mip, range);
            }
            layers_panel::Msg::SetOpacityCommit(id) => {
                self.layers_panel.update(&m);
                let session_id = self.state.active_session().map(|t| t.id);
                let before = self
                    .state
                    .active_session()
                    .and_then(|t| t.document.find_layer(*id).map(|l| l.blend.opacity))
                    .unwrap_or(1.0);
                let after = self
                    .layers_panel
                    .pending_opacity
                    .and_then(|(pid, o)| if pid == *id { Some(o) } else { None })
                    .unwrap_or(before);
                self.layers_panel.pending_opacity = None;
                if let Some(session_id) = session_id {
                    if let Some(ref prev) = self.pending_preview {
                        prev.undo(&mut self.state.session_mut(session_id).unwrap().document);
                        self.pending_preview = None;
                    }
                    let mutation: Arc<dyn pixors_document::mutation::Mutation> = Arc::new(
                        pixors_document::mutation::impls::SetLayerOpacity {
                            tab: session_id,
                            layer: *id,
                            before,
                            after,
                        },
                    );
                    let _ = self.dispatcher.commit(mutation, &mut self.state);
                    let (mip, range) = self.viewport_mip_range(session_id, 1);
                    self.run_render(session_id, mip, range);
                }
            }
            _ => {}
        }

        let drag_from = self.layers_panel.drag_from;
        let drag_over = self.layers_panel.drag_over;
        self.layers_panel.update(&m);
        let layers = self
            .state
            .active_session()
            .map(|t| t.document.layers.as_slice())
            .unwrap_or(&[]);
        let ctx = layers_panel::LayersContext {
            active_tab_id: self.state.active_session().map(|t| t.id),
            layers,
            drag_from,
            drag_over,
        };
        let effects = layers_panel::update(m, ctx);
        self.execute_effects(effects);
    }

    pub(crate) fn recomposite_current_view(&mut self, session_id: SessionId) {
        let (mip, range) = self.viewport_mip_range(session_id, 3);
        self.run_render(session_id, mip, range);
    }
}
