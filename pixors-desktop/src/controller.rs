use pixors_document::TabId;
use std::sync::Arc;

use iced::keyboard::{self, Key};
use iced::widget::pane_grid;
use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::{AlphaPolicy, PixelFormat};
use pixors_engine::data::buffer::Buffer;
use pixors_engine::data::tile::{Tile, TileCoord};
use pixors_engine::data_transform::to_neighborhood::TileToNeighborhood;
use pixors_engine::graph::graph::{EdgePorts, ExecGraph};
use pixors_engine::graph::item::Item;
use pixors_engine::runtime::event::PipelineEvent;
use pixors_engine::stage::Stage;
use pixors_ops::source::cache_reader::{CacheReader, TileRange};

use pixors_ops::processor::color::ColorConvert;
use pixors_engine::common::pixel::meta::PixelMeta;
use pixors_document::action::PipelineMode;
use pixors_document::PathBuilder;
use pixors_ops::processor::blur::Blur;
use pixors_ops::processor::compose::Compose;

use crate::app::{App, Msg, PaneKind};
use crate::page::editor::tab_bar;
use crate::page::editor::toolbar::Tool;
use crate::page::menu_bar;
use crate::panel::{filter as filters_panel, layers as layers_panel};
use crate::viewport::tile_cache::{CachedTile, TileCache};
use crate::viewport::tile_cache_sink::{TileCacheSink, register_tile_cache, unregister_tile_cache};
use crate::viewport::tile_cache_source::{
    TileCacheSource, install_tile_cache_reader, uninstall_tile_cache_reader,
};
use crate::viewport::viewport_state::ViewportState;
use pixors_engine::data::tile::TileGridPos;
use pixors_document::TILE_SIZE;

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
                    if self.state.tab(tab_id).is_some()
                        && !self.tile_caches.contains_key(&tab_id)
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
                    // Desktop cleanup first, before state removes the tab.
                    self.tile_caches.remove(&id);
                    self.viewport_states.remove(&id);
                    self.mip_queues.remove(&id);
                    unregister_tile_cache(id.0);
                    uninstall_tile_cache_reader(id.0);

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
            Msg::NewFilterPanel(m) => {
                if let Some(forwarded_msg) = self.new_filter.update(m) {
                    self.handle_filters_msg(forwarded_msg);
                }
            }
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
                Arc::new(pixors_document::action::actions::open_file::OpenFile::new(path)),
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
        let cache_dir = tab.session.cache_dir.clone();
        let display_format = self.state.display_format;
        let display_color_space = self.state.display_color_space;

        let tile_cache = TileCache::new();

        // Register sink callback (pipeline → RAM cache).
        {
            let cache = tile_cache.clone();
            register_tile_cache(
                tab_id.0,
                Box::new(move |generation, mip, tx, ty, px, py, tw, th, bytes| {
                    if let Ok(mut guard) = cache.lock() {
                        guard.insert(
                            generation,
                            TileGridPos { mip_level: mip, tx, ty },
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

        // Register source callback (BlurPreview reads base tiles from RAM cache).
        {
            let cache = tile_cache.clone();
            install_tile_cache_reader(
                tab_id.0,
                Box::new(move |_key, generation, mip, _range| {
                    let guard = cache.lock().unwrap();
                    guard
                        .tiles_at_mip(mip, generation)
                        .into_iter()
                        .map(|(pos, ct)| {
                            Item::Tile(Tile::new(
                                TileCoord {
                                    mip_level: pos.mip_level,
                                    tx: pos.tx,
                                    ty: pos.ty,
                                    px: ct.px,
                                    py: ct.py,
                                    width: ct.width,
                                    height: ct.height,
                                    tile_size: TILE_SIZE,
                                    image_width: img_w,
                                    image_height: img_h,
                                },
                                PixelMeta::new(
                                    display_format,
                                    display_color_space,
                                    AlphaPolicy::Straight,
                                ),
                                Buffer::cpu(ct.bytes.as_ref().clone()),
                            ))
                        })
                        .collect()
                }),
            );
        }

        tile_cache.lock().unwrap().signal_new_img(img_w, img_h);

        let mut vs = ViewportState::default();
        vs.camera.img_w = img_w as f32;
        vs.camera.img_h = img_h as f32;

        let mip_queue = Arc::new(std::sync::Mutex::new(Vec::new()));

        self.tile_caches.insert(tab_id, tile_cache);
        self.viewport_states
            .insert(tab_id, Arc::new(std::sync::RwLock::new(vs)));
        self.mip_queues.insert(tab_id, mip_queue);

        // Trigger full mip-0 fetch so tiles appear immediately.
        let ntx = img_w.div_ceil(TILE_SIZE);
        let nty = img_h.div_ceil(TILE_SIZE);
        let full_range = TileRange { tx_start: 0, tx_end: ntx, ty_start: 0, ty_end: nty };
        self.run_mip_fetch(tab_id, 0, full_range);
    }

    pub(crate) fn handle_tick(&mut self) {
        self.errors.retain(|(_, ts)| ts.elapsed().as_secs() < 5);

        let mut mip_requests: Vec<(TabId, u32, TileRange)> = Vec::new();

        for tab in &mut self.state.tabs {
            if let Some(cache) = self.tile_caches.get(&tab.id)
                && cache.lock().is_ok_and(|g| g.has_pending())
            {
                tab.session.redraw_seq = tab.session.redraw_seq.wrapping_add(1);
            }

            if let Some(queue) = self.mip_queues.get(&tab.id) {
                let mut sigs = queue.lock().unwrap();
                if !sigs.is_empty() {
                    for (tab_id, mip, range) in sigs.drain(..) {
                        mip_requests.push((tab_id, mip, range));
                    }
                }
            }
        }

        for (tab_id, mip, range) in mip_requests {
            if let Some(cache) = self.tile_caches.get(&tab_id)
                && let Ok(guard) = cache.lock()
                && guard.has_all_tiles(mip, &range)
            {
                continue;
            }
            self.run_mip_fetch(tab_id, mip, range);
        }
    }

    fn run_mip_fetch(&mut self, tab_id: TabId, mip: u32, range: TileRange) {
        let Some(tab) = self.state.tab(tab_id) else { return };

        let visible: Vec<&pixors_document::LayerNode> = tab.document.visible_layers();
        if visible.is_empty() { return; }

        let mut graph = ExecGraph::new();
        let n = visible.len();

        let compose = graph.add_stage(Stage::Processor(Box::new(Compose::new(
            n as u16,
            visible.iter().map(|l| l.blend.mode).collect(),
            visible.iter().map(|l| l.blend.opacity).collect(),
        ))));

        let color_out = graph.add_stage(Stage::Processor(Box::new(ColorConvert {
            target_format: self.state.display_format,
            target_color_space: self.state.display_color_space,
            target_alpha: AlphaPolicy::Straight,
        })));
        graph.add_edge(compose, color_out, EdgePorts { from_port: 0, to_port: 0 });

        let sink = graph.add_stage(Stage::Consumer(Box::new(TileCacheSink::new(tab_id.0, 0))));
        graph.add_edge(color_out, sink, EdgePorts { from_port: 0, to_port: 0 });

        let cw = tab.document.canvas.width;
        let ch = tab.document.canvas.height;
        let (img_w, img_h) = if mip == 0 { (cw, ch) }
            else { let s = 1u32 << mip; ((cw + s - 1) / s, (ch + s - 1) / s) };

        for (i, layer) in visible.iter().enumerate() {
            let layer_cache = tab.layer_cache_dir(layer.id);
            let reader = graph.add_stage(Stage::Producer(Box::new(CacheReader {
                cache_dir: layer_cache,
                mip_level: mip,
                tile_size: TILE_SIZE,
                image_width: img_w,
                image_height: img_h,
                tile_range: Some(range.clone()),
                pixel_format: PixelFormat::RgbaF16,
                color_space: ColorSpace::ACES_CG,
            })));

            let mut prev_id = reader;
            let mut prev_port = 0u16;

            // Per-layer filter chain
            for filter in &layer.filters {
                match filter {
                    pixors_document::LayerFilter::Blur { radius } => {
                        let ttn = graph.add_stage(Stage::Processor(Box::new(
                            TileToNeighborhood::new(*radius as u32),
                        )));
                        graph.add_edge(prev_id, ttn, EdgePorts { from_port: prev_port, to_port: 0 });

                        let blur = graph.add_stage(Stage::Processor(Box::new(Blur {
                            radius: *radius as u32,
                        })));
                        graph.add_edge(ttn, blur, EdgePorts { from_port: 0, to_port: 0 });

                        prev_id = blur;
                        prev_port = 0;
                    }
                }
            }

            graph.add_edge(prev_id, compose, EdgePorts { from_port: prev_port, to_port: i as u16 });
        }

        let _ = self.dispatcher.run_graph(graph, PipelineMode::Background, Some(tab_id));
    }

    fn run_blur_preview(&mut self, tab_id: TabId, radius: u32, generation: u64, mip: u32) {
        let (img_w, img_h) = self
            .state
            .tab(tab_id)
            .map(|t| (t.document.canvas.width, t.document.canvas.height))
            .unwrap_or((1, 1));

        let graph = PathBuilder::new()
            .src(Stage::Producer(Box::new(TileCacheSource {
                routing_key: tab_id.0,
                mip_level: mip,
                generation: 0,
                tile_range: None,
            })))
            .op(Stage::Processor(Box::new(ColorConvert {
                target_format: self.state.working_format,
                target_color_space: self.state.working_color_space,
                target_alpha: AlphaPolicy::Straight,
            })))
            .data_xform(Stage::Processor(Box::new(TileToNeighborhood::new(radius))))
            .op(Stage::Processor(Box::new(Blur { radius })))
            .op(Stage::Processor(Box::new(ColorConvert {
                target_format: self.state.display_format,
                target_color_space: self.state.display_color_space,
                target_alpha: AlphaPolicy::Straight,
            })))
            .sink(Stage::Consumer(Box::new(TileCacheSink::new(tab_id.0, generation))))
            .compile();

        let _ = self
            .dispatcher
            .run_graph(graph, PipelineMode::Background, Some(tab_id));

        let _ = (img_w, img_h); // used indirectly via source callback registration
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
        match m {
            layers_panel::Msg::Close => self.toggle_pane(PaneKind::Layers),
            layers_panel::Msg::Select(id) => {
                if let Some(tab) = self.state.active_tab_mut() {
                    tab.session.active_node = Some(id);
                }
            }
            layers_panel::Msg::ToggleVisibility(id) => {
                if let Some(tab) = self.state.active_tab() {
                    let visible = tab.document.find_layer(id).map(|l| l.visible).unwrap_or(true);
                    let _ = self.dispatcher.dispatch(
                        Arc::new(pixors_document::mutation::impls::SetLayerVisible {
                            tab: tab.id, layer: id, before: visible, after: !visible,
                        }),
                        &mut self.state,
                    );
                }
            }
            layers_panel::Msg::SetOpacity(id, opacity) => {
                if let Some(tab) = self.state.active_tab() {
                    let before = tab.document.find_layer(id).map(|l| l.blend.opacity).unwrap_or(1.0);
                    let _ = self.dispatcher.dispatch(
                        Arc::new(pixors_document::mutation::impls::SetLayerOpacity {
                            tab: tab.id, layer: id, before, after: opacity,
                        }),
                        &mut self.state,
                    );
                }
            }
        }
    }

    pub(crate) fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        match m {
            filters_panel::Msg::Close => self.toggle_pane(PaneKind::Filters),
            filters_panel::Msg::SetBlur(v) => {
                let info = self.state.active_tab_mut().and_then(|tab| {
                    let active_id = tab.session.active_node?;
                    let layer = tab.document.layers.iter_mut().find(|l| l.id == active_id)?;
                    let current = layer.filters.iter().find_map(|f| match f {
                        pixors_document::LayerFilter::Blur { radius } => Some(*radius),
                    }).unwrap_or(0.0);
                    if (current - v).abs() < 0.01 { return None; }
                    layer.filters.retain(|f| !matches!(f, pixors_document::LayerFilter::Blur { .. }));
                    layer.filters.push(pixors_document::LayerFilter::Blur { radius: v });
                    tab.session.redraw_seq = tab.session.redraw_seq.wrapping_add(1);
                    Some((tab.id, tab.session.redraw_seq))
                });
                if let Some((tab_id, generation)) = info {
                    let mip = self
                        .viewport_states
                        .get(&tab_id)
                        .and_then(|vs| vs.read().ok())
                        .map(|vs| vs.current_mip)
                        .unwrap_or(0);
                    self.run_blur_preview(tab_id, v as u32, generation, mip);
                }
            }
            filters_panel::Msg::CancelPreview => {
                self.dispatch_blur_cancel();
            }
            filters_panel::Msg::OpenFilterSearch => {
                self.show_filter_search = true;
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
            .viewport_states
            .get(&tab_id)
            .and_then(|vs| vs.read().ok())
            .map(|vs| {
                let m = vs.current_mip;
                let r = vs.camera.padded_tile_range(m, TILE_SIZE, 1);
                (m, r)
            })
            .unwrap_or((0, TileRange { tx_start: 0, tx_end: 0, ty_start: 0, ty_end: 0 }));

        // Clear overlay tiles directly — no pipeline needed.
        if let Some(cache) = self.tile_caches.get(&tab_id)
            && let Ok(mut guard) = cache.lock()
        {
            guard.clear_generation(generation);
        }

        // Re-queue the current mip to restore base tiles on screen.
        if let Some(queue) = self.mip_queues.get(&tab_id)
            && let Ok(mut sigs) = queue.lock()
        {
            sigs.push((tab_id, mip, range));
        }
    }

    pub(crate) fn push_error(&mut self, msg: String) {
        self.errors.push((msg, std::time::Instant::now()));
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
