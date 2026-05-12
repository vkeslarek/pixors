use std::path::PathBuf;
use std::sync::Arc;

use pixors_document::action::PipelineMode;
use pixors_document::render::compiler::{CompileConfig, RenderRequest, compile_preview};
use pixors_document::{NodeId, Operation, SessionId, TILE_SIZE};
use pixors_engine::stage::Stage;
use pixors_ops::source::cache_reader::TileRange;

use crate::app::App;
use crate::panel::filter as filters_panel;
use crate::viewport::tile_cache_sink::TileCacheSink;

impl App {
    pub(crate) fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        self.filter_panel.update(&m);

        match m {
            filters_panel::Msg::SetBlur(v) => {
                self.blur_preview_radius = Some(v);
                let info = self.state.active_session_mut().map(|tab| {
                    tab.transient.redraw_seq = tab.transient.redraw_seq.wrapping_add(1);
                    (tab.id, tab.transient.redraw_seq)
                });
                if let Some((session_id, generation)) = info {
                    self.dispatcher.cancel_background(session_id);
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
                            let extra_pad = (v as u32).div_ceil(TILE_SIZE) + 1;
                            let r = vs.camera.padded_tile_range(m, TILE_SIZE, 3 + extra_pad);
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
                    self.run_blur_preview(session_id, v as u32, generation, mip, range);
                }
            }
            filters_panel::Msg::CommitBlur(v) => {
                self.blur_preview_radius = None;
                let radius = v;
                let action = self.state.active_session_mut().and_then(|tab| {
                    let session_id = tab.id;
                    let layer_id = tab.transient.active_node?;
                    let layer = tab.document.layers.iter().find(|l| l.id == layer_id)?;
                    let a: Arc<dyn pixors_document::action::Action> = if let Some(existing) = layer
                        .transforms
                        .iter()
                        .find(|t| matches!(t.op, pixors_document::Operation::Blur { .. }))
                    {
                        Arc::new(pixors_document::mutation::impls::UpdateTransformOp {
                            tab: session_id,
                            layer: layer_id,
                            transform_id: existing.id,
                            before: existing.op.clone(),
                            after: pixors_document::Operation::Blur { radius },
                        })
                    } else {
                        let new_id = tab.document.alloc_node_id();
                        Arc::new(pixors_document::mutation::impls::AddTransform {
                            tab: session_id,
                            layer: layer_id,
                            transform: pixors_document::Transform {
                                id: new_id,
                                op: pixors_document::Operation::Blur { radius },
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
                    };
                    Some(a)
                });
                if let Some(a) = action {
                    let session_id = a.target_tab();
                    let _ = self.dispatcher.dispatch(a, &mut self.state);
                    if let Some(tid) = session_id {
                        // Cancel any in-flight preview pipeline and drop overlay tiles —
                        // otherwise stale preview overlay masks subsequent base updates
                        // (e.g. opacity slider) since cache.get() prefers overlay over base.
                        self.dispatcher.cancel_background(tid);
                        if let Some(cache) = self.viewport_tabs.get(&tid).map(|vt| &vt.cache)
                            && let Ok(mut guard) = cache.lock()
                        {
                            guard.active_generation = 0;
                        }
                        self.recomposite_current_view(tid);
                    }
                }
            }
            filters_panel::Msg::CancelPreview => {
                self.blur_preview_radius = None;
                self.dispatch_blur_cancel();
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
                    self.filter_panel.drag_from,
                    self.filter_panel.drag_over,
                );
                let effects = filters_panel::update(other, ctx);
                self.execute_effects(effects);
            }
        }
    }

    fn run_blur_preview(
        &mut self,
        session_id: SessionId,
        radius: u32,
        generation: u64,
        mip: u32,
        range: TileRange,
    ) {
        let (img_w, img_h, cache_dir, active_layer, df, dcs, wf, wcs) = self
            .state
            .session(session_id)
            .and_then(|t| {
                Some((
                    t.document.canvas.width,
                    t.document.canvas.height,
                    t.transient.cache_dir.clone(),
                    t.transient.active_node?,
                    t.display_format,
                    t.display_color_space,
                    t.working_format,
                    t.working_color_space,
                ))
            })
            .unwrap_or((
                1,
                1,
                PathBuf::new(),
                NodeId(0),
                pixors_engine::common::pixel::PixelFormat::Rgba8,
                pixors_engine::common::color::space::ColorSpace::SRGB,
                pixors_engine::common::pixel::PixelFormat::RgbaF16,
                pixors_engine::common::color::space::ColorSpace::ACES_CG,
            ));

        let config = CompileConfig {
            cache_dir,
            display_format: df,
            display_color_space: dcs,
            working_format: wf,
            working_color_space: wcs,
            tile_size: TILE_SIZE,
            img_w,
            img_h,
        };

        let req = RenderRequest {
            viewport: range,
            mip_level: mip,
            up_to: None,
        };

        let graph = compile_preview(
            &self.state.session(session_id).unwrap().document,
            &req,
            &config,
            Stage::Consumer(Box::new(TileCacheSink::new(session_id.0, generation, 0))),
            active_layer,
            &Operation::Blur {
                radius: radius as f32,
            },
        );

        let _ = self
            .dispatcher
            .run_graph(graph, PipelineMode::Background, Some(session_id));
    }

    fn dispatch_blur_cancel(&mut self) {
        let Some(tab) = self.state.active_session() else {
            return;
        };
        let session_id = tab.id;
        let generation = tab.transient.redraw_seq.wrapping_add(1);

        let (mip, range) = self
            .viewport_tabs
            .get(&session_id)
            .and_then(|vt| vt.state.read().ok())
            .map(|vs| {
                let m = vs.current_mip;
                let r = vs.camera.padded_tile_range(m, TILE_SIZE, 1);
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

        // Clear overlay tiles directly — no pipeline needed.
        if let Some(cache) = self.viewport_tabs.get(&session_id).map(|vt| &vt.cache)
            && let Ok(mut guard) = cache.lock()
        {
            guard.active_generation = 0;
        }

        // Re-queue the current mip to restore base tiles on screen.
        if let Some(queue) = self.viewport_tabs.get(&session_id).map(|vt| &vt.mip_queue)
            && let Ok(mut sigs) = queue.lock()
        {
            sigs.push((session_id, mip, range));
        }
    }
}
