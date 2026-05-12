use pixors_document::TabId;
use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard::{self, Key};
use iced::widget::pane_grid;
use pixors_engine::runtime::event::PipelineEvent;
use pixors_engine::stage::Stage;
use pixors_ops::source::cache_reader::TileRange;

use pixors_document::action::PipelineMode;
use pixors_document::render::compiler::{CompileConfig, RenderRequest, compile, compile_preview};
use pixors_document::{NodeId, Operation};

use crate::app::{App, Msg, PaneKind};
use crate::effect::Effect;
use crate::page::editor::tab_bar;
use crate::page::editor::toolbar::Tool;
use crate::page::menu_bar;
use crate::panel::{filter as filters_panel, layers as layers_panel};
use crate::viewport::tile_cache::{CachedTile, TileCache};
use crate::viewport::tile_cache_sink::{TileCacheSink, register_tile_cache, unregister_tile_cache};
use crate::viewport::viewport_state::ViewportState;
use pixors_document::TILE_SIZE;
use pixors_engine::data::tile::TileGridPos;

impl App {
    pub(crate) fn find_pane(&self, kind: PaneKind) -> Option<pane_grid::Pane> {
        self.panes
            .iter()
            .find_map(|(p, k)| if *k == kind { Some(*p) } else { None })
    }

    pub(crate) fn restore_or_create(&mut self, kind: PaneKind) {
        if self.find_pane(kind).is_some() {
            return;
        }
        let target = self.panes.iter().next().map(|(p, _)| *p);
        if let Some(target) = target {
            let _ = self.panes.split(pane_grid::Axis::Horizontal, target, kind);
        } else {
            let (state, _) = pane_grid::State::new(kind);
            self.panes = state;
        }
    }

    pub(crate) fn toggle_pane(&mut self, kind: PaneKind) {
        if let Some(p) = self.find_pane(kind) {
            let _ = self.panes.close(p);
        } else {
            self.restore_or_create(kind);
        }
    }

    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Action(action) => {
                if let Err(e) = self.dispatcher.dispatch(action, &mut self.state) {
                    self.push_error(e);
                }
            }
            Msg::KeyPressed(event) => self.handle_keyboard(event),
            Msg::OpenFile => self.open_file_dialog(),
            Msg::Tick => self.handle_tick(),
            Msg::Frames => {}
            Msg::PipelineEvent(e) => match e {
                PipelineEvent::Progress { tag, done, total } => {
                    let p = if total > 0 {
                        done as f32 / total as f32
                    } else {
                        1.0
                    };
                    let tab_id = TabId(tag);
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.session.view.progress = p;
                    }
                }
                PipelineEvent::Done { tag } => {
                    let tab_id = TabId(tag);
                    self.dispatcher.on_pipeline_done(&mut self.state, tab_id);
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.session.view.loading = false;
                        tab.session.view.progress = 1.0;
                    }
                    // If this tab has no viewport state yet, it was just opened.
                    if self.state.tab(tab_id).is_some() && !self.viewport_tabs.contains_key(&tab_id)
                    {
                        self.init_viewport_for_tab(tab_id);
                    }
                }
                PipelineEvent::Error { tag, message } => {
                    let tab_id = TabId(tag);
                    self.dispatcher
                        .on_pipeline_error(&mut self.state, tab_id, message.clone());
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.session.view.loading = false;
                    }
                    self.push_error(message);
                }
                PipelineEvent::Cancelled { tag } => {
                    let tab_id = TabId(tag);
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.session.view.loading = false;
                    }
                }
            },
            Msg::PipelineLagged(skipped) => {
                tracing::warn!(
                    "pipeline event channel lagged, skipped={skipped}; resyncing tab locks"
                );
                self.dispatcher.resync_locks(&mut self.state);
            }
            Msg::ExportDialog(m) => self.handle_export_dialog(m),
            Msg::UiShowcase(m) => match m {
                crate::modal::ui_showcase::Msg::Close => self.show_ui_showcase = false,
                other => self.ui_showcase.update(other),
            },
            Msg::FilterSearch(m) => match m {
                crate::modal::filter_search::Msg::Close => self.show_filter_search = false,
                crate::modal::filter_search::Msg::Apply(idx) => {
                    let op = self
                        .filter_search
                        .items
                        .get(idx)
                        .map(|item| item.op.clone())
                        .unwrap_or(pixors_document::Operation::Blur { radius: 5.0 });
                    self.filter_search
                        .update(crate::modal::filter_search::Msg::Apply(idx));
                    self.show_filter_search = false;

                    if let (Some(tab), Some(layer_id)) = (
                        self.state.active_tab(),
                        self.state.active_tab().and_then(|t| t.session.active_node),
                    ) {
                        let tab_id = tab.id;
                        let new_id = self
                            .state
                            .tab_mut(tab_id)
                            .map(|t| t.document.alloc_node_id())
                            .unwrap_or(pixors_document::NodeId(0));
                        let _ = self.dispatcher.dispatch(
                            Arc::new(pixors_document::mutation::impls::AddTransform {
                                tab: tab_id,
                                layer: layer_id,
                                transform: pixors_document::Transform {
                                    id: new_id,
                                    op,
                                    input: pixors_document::InputScope::Layer,
                                    output: pixors_document::OutputMode::Replace {
                                        blend: pixors_document::BlendSpec {
                                            mode: pixors_image::image::BlendMode::Normal,
                                            opacity: 1.0,
                                        },
                                    },
                                    enabled: true,
                                },
                            }),
                            &mut self.state,
                        );
                        self.recomposite_current_view(tab_id);
                    }
                }
                other => self.filter_search.update(other),
            },
            Msg::MenuBar(m) => self.handle_menu_msg(m),
            Msg::WorkspaceBar(m) => self.workspace.update(m),
            Msg::Toolbar(m) => {
                self.tools.update(m);
                self.status.active_tool = self.tools.active_tool;
            }
            Msg::TabBar(m) => match m {
                tab_bar::Msg::Select(id) => {
                    self.dispatcher.mutate(&mut self.state, |s| s.switch(id));
                    self.update_status_from_active_tab();
                }
                tab_bar::Msg::Close(id) => {
                    self.viewport_tabs.remove(&id);
                    unregister_tile_cache(id.0);

                    if let Err(e) = self.dispatcher.dispatch(
                        Arc::new(pixors_document::action::actions::close_tab::CloseTab(id)),
                        &mut self.state,
                    ) {
                        self.push_error(e);
                    }
                    self.dispatcher.cleanup_tab(id);
                    self.update_status_from_active_tab();
                }
                tab_bar::Msg::DragDrop => {
                    if let (Some(from), Some(to)) = (self.tabs.drag_from, self.tabs.drag_over)
                        && from != to
                    {
                        self.state.swap_tabs(from, to);
                    }
                    self.tabs.drag_from = None;
                    self.tabs.drag_over = None;
                }
                _ => self.tabs.update(m, self.state.tabs().len()),
            },
            Msg::LayersPanel(m) => self.handle_layers_msg(m),
            Msg::FiltersPanel(m) => self.handle_filters_msg(m),
            Msg::PaneResized(e) => self.panes.resize(e.split, e.ratio),
            Msg::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
            }
            Msg::PaneDragged(_) => {}
            Msg::ClosePane(pane) => {
                let _ = self.panes.close(pane);
            }
        }
    }

    pub(crate) fn handle_keyboard(&mut self, event: keyboard::Event) {
        if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
            if modifiers.contains(keyboard::Modifiers::CTRL) {
                match key.as_ref() {
                    Key::Character("o") => self.open_file_dialog(),
                    Key::Character("e") if self.active_file_path().is_some() => {
                        self.show_export_dialog = true;
                    }
                    _ => {}
                }
            } else {
                match key.as_ref() {
                    Key::Character("v") => self.tools.select(Tool::Move),
                    Key::Character("m") => self.tools.select(Tool::Select),
                    Key::Character("l") => self.tools.select(Tool::Lasso),
                    Key::Character("w") => self.tools.select(Tool::Wand),
                    Key::Character("c") => self.tools.select(Tool::Crop),
                    Key::Character("i") => self.tools.select(Tool::Eyedropper),
                    Key::Character("b") => self.tools.select(Tool::Brush),
                    Key::Character("e") => self.tools.select(Tool::Eraser),
                    Key::Character("j") => self.tools.select(Tool::Heal),
                    Key::Character("g") => self.tools.select(Tool::Gradient),
                    Key::Character("t") => self.tools.select(Tool::Text),
                    Key::Character("u") => self.tools.select(Tool::Shape),
                    Key::Character("h") => self.tools.select(Tool::Hand),
                    Key::Character("z") => self.tools.select(Tool::Zoom),
                    _ => {}
                }
            }
            self.status.active_tool = self.tools.active_tool;
        }
    }

    pub(crate) fn open_file_dialog(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("Images", &["png", "tiff", "tif"])
            .pick_file();

        if let Some(path) = path {
            if let Err(e) = self.dispatcher.dispatch(
                Arc::new(pixors_document::action::actions::open_file::OpenFile::new(
                    path,
                )),
                &mut self.state,
            ) {
                self.push_error(e);
            } else {
                self.update_status_from_active_tab();
            }
        }
    }

    /// Create viewport state for a newly opened tab and trigger initial MipFetch.
    fn init_viewport_for_tab(&mut self, tab_id: TabId) {
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
                Box::new(move |generation, mip, tx, ty, px, py, tw, th, bytes| {
                    if let Ok(mut guard) = cache.lock() {
                        guard.insert(
                            generation,
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
                }),
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

    pub(crate) fn handle_tick(&mut self) {
        self.errors.retain(|(_, ts)| ts.elapsed().as_secs() < 5);

        let mut mip_requests: Vec<(TabId, u32, TileRange)> = Vec::new();

        for tab in &mut self.state.tabs {
            if let Some(cache) = self.viewport_tabs.get(&tab.id).map(|vt| &vt.cache)
                && cache.lock().is_ok_and(|g| g.has_pending())
            {
                tab.session.redraw_seq = tab.session.redraw_seq.wrapping_add(1);
            }

            if let Some(queue) = self.viewport_tabs.get(&tab.id).map(|vt| &vt.mip_queue) {
                let mut sigs = queue.lock().unwrap();
                if !sigs.is_empty() {
                    for (tab_id, mip, range) in sigs.drain(..) {
                        mip_requests.push((tab_id, mip, range));
                    }
                }
            }
        }

        for (tab_id, mip, range) in mip_requests {
            if let Some(cache) = self.viewport_tabs.get(&tab_id).map(|vt| &vt.cache)
                && let Ok(guard) = cache.lock()
                && guard.has_all_tiles(mip, &range)
            {
                continue;
            }
            self.run_mip_fetch(tab_id, mip, range);
        }
    }

    fn run_mip_fetch(&mut self, tab_id: TabId, mip: u32, range: TileRange) {
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
        let sink = Stage::Consumer(Box::new(TileCacheSink::new(tab_id.0, 0)));
        let graph = compile(&tab.document, &req, &config, sink);

        let _ = self
            .dispatcher
            .run_graph(graph, PipelineMode::Background, Some(tab_id));
    }

    fn run_blur_preview(&mut self, tab_id: TabId, radius: u32, generation: u64, mip: u32) {
        let (img_w, img_h, cache_dir, active_layer) = self
            .state
            .tab(tab_id)
            .and_then(|t| {
                Some((
                    t.document.canvas.width,
                    t.document.canvas.height,
                    t.session.cache_dir.clone(),
                    t.session.active_node?,
                ))
            })
            .unwrap_or((1, 1, PathBuf::new(), NodeId(0)));

        let mip_scale = 1u32 << mip;
        let mip_w = img_w.div_ceil(mip_scale);
        let mip_h = img_h.div_ceil(mip_scale);

        let config = CompileConfig {
            cache_dir,
            display_format: self.state.display_format,
            display_color_space: self.state.display_color_space,
            working_format: self.state.working_format,
            working_color_space: self.state.working_color_space,
            tile_size: TILE_SIZE,
            img_w,
            img_h,
        };

        let req = RenderRequest {
            viewport: TileRange {
                tx_start: 0,
                tx_end: mip_w.div_ceil(TILE_SIZE),
                ty_start: 0,
                ty_end: mip_h.div_ceil(TILE_SIZE),
            },
            mip_level: mip,
            up_to: None,
        };

        let graph = compile_preview(
            &self.state.tab(tab_id).unwrap().document,
            &req,
            &config,
            Stage::Consumer(Box::new(TileCacheSink::new(tab_id.0, generation))),
            active_layer,
            &Operation::Blur {
                radius: radius as f32,
            },
        );

        let _ = self
            .dispatcher
            .run_graph(graph, PipelineMode::Background, Some(tab_id));
    }

    pub(crate) fn handle_menu_msg(&mut self, m: menu_bar::Msg) {
        match m {
            menu_bar::Msg::Exit => std::process::exit(0),
            menu_bar::Msg::ToggleLayers => self.toggle_pane(PaneKind::Layers),
            menu_bar::Msg::ToggleFilters => self.toggle_pane(PaneKind::Filters),
            menu_bar::Msg::ShowUiShowcase => self.show_ui_showcase = true,
            menu_bar::Msg::ResetLayout => {
                self.panes = Self::default().panes;
            }
            menu_bar::Msg::OpenFile => self.open_file_dialog(),
            menu_bar::Msg::Export if self.active_file_path().is_some() => {
                self.show_export_dialog = true;
            }
            _ => {}
        }
    }

    pub(crate) fn handle_export_dialog(&mut self, m: crate::modal::export::Msg) {
        match m {
            crate::modal::export::Msg::Export => {
                let config = self.export_dialog.encoder_config();
                let ext = self.export_dialog.file_extension();
                self.show_export_dialog = false;

                if let Some(ref path) = self.active_file_path().map(|p| p.to_path_buf()) {
                    let suggested = path.with_extension(ext);
                    if let Some(save_path) = rfd::FileDialog::new()
                        .add_filter(ext.to_uppercase().as_str(), &[ext])
                        .set_file_name(
                            suggested
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("export"),
                        )
                        .save_file()
                    {
                        let Some(tab) = self.state.active_tab_mut() else {
                            return;
                        };
                        let tab_id = tab.id;
                        tab.session.view.loading = true;
                        tab.session.view.progress = 0.0;

                        let action = Arc::new(pixors_document::action::actions::export::Export {
                            tab: tab_id,
                            source_path: path.clone(),
                            save_path: save_path.clone(),
                            config: config.clone(),
                            dpi: None,
                            icc_profile: None,
                            image_height: tab.document.canvas.height,
                        });

                        if let Err(e) = self.dispatcher.dispatch(action, &mut self.state) {
                            self.push_error(e);
                        }
                    }
                }
            }
            crate::modal::export::Msg::Cancel => {
                self.show_export_dialog = false;
            }
            other => self.export_dialog.update(other),
        }
    }

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

    fn recomposite_current_view(&mut self, tab_id: TabId) {
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

    pub(crate) fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        self.filter_panel.update(&m);

        match m {
            filters_panel::Msg::SetBlur(v) => {
                self.blur_preview_radius = Some(v);
                let info = self.state.active_tab_mut().map(|tab| {
                    tab.session.redraw_seq = tab.session.redraw_seq.wrapping_add(1);
                    (tab.id, tab.session.redraw_seq)
                });
                if let Some((tab_id, generation)) = info {
                    self.dispatcher.cancel_background(tab_id);
                    let mip = self
                        .viewport_tabs
                        .get(&tab_id)
                        .and_then(|vt| vt.state.read().ok())
                        .map(|vs| vs.current_mip)
                        .unwrap_or(0);
                    self.run_blur_preview(tab_id, v as u32, generation, mip);
                }
            }
            filters_panel::Msg::CommitBlur(v) => {
                self.blur_preview_radius = None;
                let radius = v;
                let action = self.state.active_tab_mut().and_then(|tab| {
                    let tab_id = tab.id;
                    let layer_id = tab.session.active_node?;
                    let layer = tab.document.layers.iter().find(|l| l.id == layer_id)?;
                    let a: Arc<dyn pixors_document::action::Action> = if let Some(existing) = layer
                        .transforms
                        .iter()
                        .find(|t| matches!(t.op, pixors_document::Operation::Blur { .. }))
                    {
                        Arc::new(pixors_document::mutation::impls::UpdateTransformOp {
                            tab: tab_id,
                            layer: layer_id,
                            transform_id: existing.id,
                            before: existing.op.clone(),
                            after: pixors_document::Operation::Blur { radius },
                        })
                    } else {
                        let new_id = tab.document.alloc_node_id();
                        Arc::new(pixors_document::mutation::impls::AddTransform {
                            tab: tab_id,
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
                    let tab_id = a.target_tab();
                    let _ = self.dispatcher.dispatch(a, &mut self.state);
                    if let Some(tid) = tab_id {
                        self.recomposite_current_view(tid);
                    }
                }
            }
            filters_panel::Msg::CancelPreview => {
                self.blur_preview_radius = None;
                self.dispatch_blur_cancel();
            }
            other => {
                let tab = self.state.active_tab();
                let tab_id = tab.map(|t| t.id).unwrap_or(pixors_document::TabId(0));
                let active_layer_id = tab.and_then(|t| t.session.active_node);
                let transforms: &[pixors_document::Transform] = tab
                    .and_then(|t| {
                        t.session
                            .active_node
                            .and_then(|id| t.document.find_layer(id))
                    })
                    .map(|l| l.transforms.as_slice())
                    .unwrap_or(&[]);
                let ctx = filters_panel::FilterContext::new(
                    tab_id,
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

    fn dispatch_blur_cancel(&mut self) {
        let Some(tab) = self.state.active_tab() else {
            return;
        };
        let tab_id = tab.id;
        let generation = tab.session.redraw_seq.wrapping_add(1);

        let (mip, range) = self
            .viewport_tabs
            .get(&tab_id)
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
        if let Some(cache) = self.viewport_tabs.get(&tab_id).map(|vt| &vt.cache)
            && let Ok(mut guard) = cache.lock()
        {
            guard.clear_generation(generation);
        }

        // Re-queue the current mip to restore base tiles on screen.
        if let Some(queue) = self.viewport_tabs.get(&tab_id).map(|vt| &vt.mip_queue)
            && let Ok(mut sigs) = queue.lock()
        {
            sigs.push((tab_id, mip, range));
        }
    }

    pub(crate) fn push_error(&mut self, msg: String) {
        self.errors.push((msg, std::time::Instant::now()));
    }

    fn execute_effects(&mut self, effects: Vec<Effect>) {
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
                    tab_id,
                } => {
                    let _ = self.dispatcher.run_graph(graph, mode, tab_id);
                }
                Effect::QueueDisplayRefresh(tab_id) => {
                    self.recomposite_current_view(tab_id);
                }
                Effect::CancelBackground(tab_id) => {
                    self.dispatcher.cancel_background(tab_id);
                }
                Effect::ClearOverlay(tab_id) => {
                    if let Some(cache) = self.viewport_tabs.get(&tab_id).map(|vt| &vt.cache)
                        && let Ok(mut guard) = cache.lock()
                    {
                        let generation = self
                            .state
                            .tab(tab_id)
                            .map(|t| t.session.redraw_seq)
                            .unwrap_or(0);
                        guard.clear_generation(generation);
                    }
                }
                Effect::ShowFilterSearch => {
                    self.show_filter_search = true;
                }
                Effect::TogglePane(kind) => self.toggle_pane(kind),
                Effect::ToggleTransformEnabled {
                    tab_id,
                    layer_id,
                    transform_id,
                    enabled,
                } => {
                    if let Some(tab) = self.state.tab(tab_id)
                        && let Some(layer) = tab.document.find_layer(layer_id)
                        && let Some(t) = layer.transforms.iter().find(|t| t.id == transform_id)
                    {
                        let _ = self.dispatcher.dispatch(
                            Arc::new(pixors_document::mutation::impls::SetTransformEnabled {
                                tab: tab_id,
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
                    tab_id,
                    layer_id,
                    from,
                    to,
                } => {
                    if let Some(tab) = self.state.tab(tab_id)
                        && let Some(_layer) = tab.document.find_layer(layer_id)
                        && from < _layer.transforms.len()
                        && to < _layer.transforms.len()
                    {
                        let _ = self.dispatcher.dispatch(
                            Arc::new(pixors_document::mutation::impls::ReorderTransform {
                                tab: tab_id,
                                layer: layer_id,
                                from,
                                to,
                            }),
                            &mut self.state,
                        );
                        self.recomposite_current_view(tab_id);
                    }
                }
                Effect::PushError(msg) => self.push_error(msg),
                Effect::None => {}
            }
        }
    }

    pub(crate) fn update_status_from_active_tab(&mut self) {
        if let Some(tab) = self.state.active_tab() {
            self.status.canvas_w = tab.document.canvas.width;
            self.status.canvas_h = tab.document.canvas.height;
            self.status.layers = tab.document.layers.len();
        } else {
            self.status.canvas_w = 0;
            self.status.canvas_h = 0;
            self.status.layers = 0;
        }
    }
}
