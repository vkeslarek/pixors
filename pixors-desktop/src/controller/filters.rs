use std::sync::Arc;

use crate::app::App;
use crate::panel::filter as filters_panel;

impl App {
    pub(crate) fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        let drag_from = self.filter_panel.drag_from;
        let drag_over = self.filter_panel.drag_over;
        self.filter_panel.update(&m);

        match m {
            filters_panel::Msg::SetBlur(v) => {
                let session_id = match self.state.active_session().map(|t| t.id) {
                    Some(id) => id,
                    None => return,
                };

                let Some(tab) = self.state.active_session_mut() else {
                    return;
                };
                let layer_id = match tab.transient.active_node {
                    Some(id) => id,
                    None => return,
                };
                let existing = tab
                    .document
                    .layers
                    .iter()
                    .find(|l| l.id == layer_id)
                    .and_then(|l| {
                        l.transforms
                            .iter()
                            .find(|t| matches!(t.op, pixors_document::Operation::Blur { .. }))
                            .and_then(|t| match t.op {
                                pixors_document::Operation::Blur { radius } => Some((t.id, radius)),
                                _ => None,
                            })
                    });

                let mutation: Arc<dyn pixors_document::mutation::Mutation> = match existing {
                    Some((transform_id, before)) => {
                        Arc::new(pixors_document::mutation::impls::UpdateTransformOp {
                            tab: session_id,
                            layer: layer_id,
                            transform_id,
                            before: pixors_document::Operation::Blur { radius: before },
                            after: pixors_document::Operation::Blur { radius: v },
                        })
                    }
                    None => {
                        let new_id = tab.document.alloc_node_id();
                        Arc::new(pixors_document::mutation::impls::AddTransform {
                            tab: session_id,
                            layer: layer_id,
                            transform: pixors_document::Transform {
                                id: new_id,
                                op: pixors_document::Operation::Blur { radius: v },
                                input: pixors_document::InputScope::Layer,
                                output: pixors_document::OutputMode::Replace {
                                    blend: pixors_document::BlendSpec {
                                        mode: pixors_image::image::BlendMode::Normal,
                                        opacity: 1.0,
                                    },
                                },
                                enabled: true,
                            },
                        })
                    }
                };

                if let Some(ref prev) = self.pending_preview {
                    prev.undo(&mut self.state.session_mut(session_id).unwrap().document);
                }
                mutation.apply(&mut self.state.session_mut(session_id).unwrap().document);
                self.pending_preview = Some(mutation.clone());
                let _ = self.dispatcher.preview(mutation, &mut self.state);

                let (mip, range) = self.viewport_mip_range(session_id, 3);
                self.run_render(session_id, mip, range);
            }
            filters_panel::Msg::CommitBlur(v) => {
                let session_id = match self.state.active_session().map(|t| t.id) {
                    Some(id) => id,
                    None => return,
                };

                if let Some(ref prev) = self.pending_preview {
                    prev.undo(&mut self.state.session_mut(session_id).unwrap().document);
                    self.pending_preview = None;
                }

                let Some(tab) = self.state.active_session_mut() else {
                    return;
                };
                let layer_id = match tab.transient.active_node {
                    Some(id) => id,
                    None => return,
                };
                let existing = tab
                    .document
                    .layers
                    .iter()
                    .find(|l| l.id == layer_id)
                    .and_then(|l| {
                        l.transforms
                            .iter()
                            .find(|t| matches!(t.op, pixors_document::Operation::Blur { .. }))
                            .and_then(|t| match t.op {
                                pixors_document::Operation::Blur { radius } => Some((t.id, radius)),
                                _ => None,
                            })
                    });

                let mutation: Arc<dyn pixors_document::mutation::Mutation> = match existing {
                    Some((transform_id, before)) => {
                        Arc::new(pixors_document::mutation::impls::UpdateTransformOp {
                            tab: session_id,
                            layer: layer_id,
                            transform_id,
                            before: pixors_document::Operation::Blur { radius: before },
                            after: pixors_document::Operation::Blur { radius: v },
                        })
                    }
                    None => {
                        let new_id = tab.document.alloc_node_id();
                        Arc::new(pixors_document::mutation::impls::AddTransform {
                            tab: session_id,
                            layer: layer_id,
                            transform: pixors_document::Transform {
                                id: new_id,
                                op: pixors_document::Operation::Blur { radius: v },
                                input: pixors_document::InputScope::Layer,
                                output: pixors_document::OutputMode::Replace {
                                    blend: pixors_document::BlendSpec {
                                        mode: pixors_image::image::BlendMode::Normal,
                                        opacity: 1.0,
                                    },
                                },
                                enabled: true,
                            },
                        })
                    }
                };
                let _ = self.dispatcher.commit(mutation, &mut self.state);
                if let Some(tab) = self.state.session_mut(session_id) {
                    tab.transient.view.loading = true;
                    tab.transient.view.progress = 0.0;
                }
                let (mip, range) = self.viewport_mip_range(session_id, 1);
                self.run_render(session_id, mip, range);
            }
            filters_panel::Msg::CancelPreview => {
                let session_id = match self.state.active_session().map(|t| t.id) {
                    Some(id) => id,
                    None => return,
                };
                if let Some(ref prev) = self.pending_preview {
                    prev.undo(&mut self.state.session_mut(session_id).unwrap().document);
                    self.pending_preview = None;
                }
                if let Some(cache) = self.viewport_tabs.get(&session_id).map(|vt| &vt.cache)
                    && let Ok(mut guard) = cache.lock()
                {
                    guard.active_generation = 0;
                }
                if let Some(queue) = self.viewport_tabs.get(&session_id).map(|vt| &vt.mip_queue) {
                    let _ = queue.lock().map(|mut sigs| {
                        let (mip, range) = self.viewport_mip_range(session_id, 1);
                        sigs.push((session_id, mip, range));
                    });
                }
            }
            other => {
                let tab = self.state.active_session();
                let session_id = tab.map(|t| t.id).unwrap_or(pixors_document::SessionId(0));
                let active_layer_id = tab.and_then(|t| t.transient.active_node);
                let transforms: &[pixors_document::Transform] = tab
                    .and_then(|t| {
                        t.transient
                            .active_node
                            .and_then(|id| t.document.find_layer(id))
                    })
                    .map(|l| l.transforms.as_slice())
                    .unwrap_or(&[]);
                let ctx = filters_panel::FilterContext::new(
                    session_id,
                    active_layer_id,
                    transforms,
                    drag_from,
                    drag_over,
                );
                let effects = filters_panel::update(other, ctx);
                self.execute_effects(effects);
            }
        }
    }
}
