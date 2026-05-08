use crate::state::TabId;
use iced::keyboard::{self, Key};
use iced::widget::pane_grid;
use pixors_executor::runtime::event::PipelineEvent;
use pixors_executor::source::cache_reader::TileRange;
use std::sync::{Arc, Mutex};

use crate::app::{App, Msg, PaneKind};
use crate::components::{filters_panel, layers_panel, menu_bar, tab_bar};
use crate::components::toolbar::Tool;

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
            let _ = self
                .panes
                .split(pane_grid::Axis::Horizontal, target, kind);
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
            Msg::KeyPressed(event) => self.handle_keyboard(event),
            Msg::OpenFile => self.open_file_dialog(),
            Msg::Tick => self.handle_tick(),
            Msg::Frames => {} // Just to wake up the event loop
            Msg::PipelineEvent(e) => match e {
                PipelineEvent::Progress { done, total } => {
                    let p = if total > 0 { done as f32 / total as f32 } else { 1.0 };
                    for tab in &mut self.state.tabs {
                        if tab.view.loading {
                            tab.view.progress = p;
                        }
                    }
                    tracing::info!("[pixors] UI progress updated: {}/{}", done, total);
                }
                PipelineEvent::Done => {
                    for tab in &mut self.state.tabs {
                        tab.view.loading = false;
                        tab.view.progress = 1.0;
                    }
                    tracing::info!("[pixors] UI progress Done!");
                }
                PipelineEvent::Error(s) => {
                    for tab in &mut self.state.tabs {
                        tab.view.loading = false;
                    }
                    self.push_error(s);
                }
            },
            Msg::ExportDialog(m) => self.handle_export_dialog(m),
            Msg::MenuBar(m) => self.handle_menu_msg(m),
            Msg::WorkspaceBar(m) => self.workspace.update(m),
            Msg::Toolbar(m) => {
                self.tools.update(m);
                self.status.active_tool = self.tools.active_tool;
            }
            Msg::TabBar(m) => match m {
                tab_bar::Msg::Select(id) => {
                    self.state.switch(id);
                    self.update_status_from_active_tab();
                }
                tab_bar::Msg::Close(id) => {
                    self.state.close(id);
                    self.update_status_from_active_tab();
                }
                tab_bar::Msg::DragDrop => {
                    if let (Some(from), Some(to)) = (self.tabs.drag_from, self.tabs.drag_over) {
                        if from != to {
                            self.state.swap_tabs(from, to);
                        }
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
                    Key::Character("e") => {
                        if self.image_path.is_some() {
                            self.show_export_dialog = true;
                        }
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
        tracing::info!("[pixors] open_file_dialog: starting");

        let vp_cache = crate::viewport::tile_cache::ViewportCache::new();
        match crate::file_ops::open_and_run(Some(vp_cache.clone())) {
            Ok((w, h, path)) => {
                // --- backward compat: old flat fields (remove in Phase B) ---
                self.image_path = Some(path.clone());

                // --- new: create Tab in EditorState ---
                let tab_id = self.state.alloc_tab_id();
                let title = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string();

                // Re-open metadata for the descriptor (cheap, sync)
                let desc = match pixors_executor::common::image::Image::open(&path) {
                    Ok(img) => img.desc,
                    Err(_) => pixors_executor::common::image::ImageDescriptor {
                        format: String::new(),
                        width: w,
                        height: h,
                        bit_depth: 8,
                        color_space: pixors_executor::common::color::space::ColorSpace::SRGB,
                        dpi: None,
                        metadata: vec![],
                        icc_profile: None,
                        pages: vec![],
                    },
                };

                self.state.push_tab(crate::state::Tab {
                    id: tab_id,
                    title,
                    source: crate::state::TabSource::File {
                        path: path.clone(),
                    },
                    desc,
                    cache_dir: path.with_extension("pixors_cache"),
                    viewport_cache: vp_cache,
                    viewport_state: {
                        use std::cell::RefCell;
                        use std::rc::Rc;
                        use crate::viewport::state::ViewportState;
                        let mut vs = ViewportState::default();
                        vs.camera.img_w = w as f32;
                        vs.camera.img_h = h as f32;
                        Rc::new(RefCell::new(vs))
                    },
                    mip_fetch_signal: Arc::new(Mutex::new(Vec::<(TabId, u32, TileRange)>::new())),
                    tile_generation: 0,
                    layers: vec![],
                    active_layer: None,
                    chain: Default::default(),
                    history: Default::default(),
                    view: crate::state::TabView {
                        zoom: 1.0,
                        pan: (0.0, 0.0),
                        active_mip: 0,
                        loading: true,
                        progress: 0.0,
                    },
                });
                self.update_status_from_active_tab();

                self.push_error(format!(
                    "OK {}×{} — {}",
                    w,
                    h,
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) if e == "cancelled" => {}
            Err(e) => self.push_error(e),
        }
    }

    pub(crate) fn handle_tick(&mut self) {
        self.errors.retain(|(_, ts)| ts.elapsed().as_secs() < 5);

        let mut mip_requests: Vec<(TabId, u32, TileRange)> = Vec::new();

        for tab in &mut self.state.tabs {
            if tab.viewport_cache.lock().is_ok_and(|g| g.has_pending()) {
                tab.tile_generation = tab.tile_generation.wrapping_add(1);
            }

            let mut sigs = tab.mip_fetch_signal.lock().unwrap();
            if !sigs.is_empty() {
                mip_requests.extend(sigs.drain(..));
            }
        }

        for (tab_id, mip, range) in mip_requests {
            self.fetch_mip_from_cache(tab_id, mip, range);
        }
    }

    pub(crate) fn handle_menu_msg(&mut self, m: menu_bar::Msg) {
        match m {
            menu_bar::Msg::Exit => std::process::exit(0),
            menu_bar::Msg::ToggleLayers => self.toggle_pane(PaneKind::Layers),
            menu_bar::Msg::ToggleFilters => self.toggle_pane(PaneKind::Filters),
            menu_bar::Msg::ResetLayout => {
                self.panes = Self::default().panes;
            }
            menu_bar::Msg::OpenFile => self.open_file_dialog(),
            menu_bar::Msg::Export => {
                if self.image_path.is_some() {
                    self.show_export_dialog = true;
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_export_dialog(&mut self, m: crate::dialog::export::Msg) {
        match m {
            crate::dialog::export::Msg::Export => {
                let config = self.export_dialog.encoder_config();
                let ext = self.export_dialog.file_extension();
                self.show_export_dialog = false;

                if let Some(ref path) = self.image_path {
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
                        if let Some(tab) = self.state.active_tab_mut() {
                            tab.view.loading = true;
                            tab.view.progress = 0.0;
                        }
                        let save = save_path.clone();
                        let c = config.clone();
                        let tx = crate::app::pipeline_event_tx();
                        std::thread::spawn(move || {
                            match crate::file_ops::export_file(&save, c) {
                                Ok(()) => {
                                    let _ = tx.send(PipelineEvent::Done);
                                }
                                Err(e) => {
                                    let _ = tx.send(PipelineEvent::Error(e));
                                }
                            }
                        });
                    }
                }
            }
            crate::dialog::export::Msg::Cancel => {
                self.show_export_dialog = false;
            }
            other => self.export_dialog.update(other),
        }
    }

    pub(crate) fn handle_layers_msg(&mut self, m: layers_panel::Msg) {
        match m {
            layers_panel::Msg::Close => self.toggle_pane(PaneKind::Layers),
            layers_panel::Msg::Select(i) => self.layers.active = i,
            layers_panel::Msg::ToggleVisibility(i) => {
                if let Some(layer) = self.layers.layers.get_mut(i) {
                    layer.visible = !layer.visible;
                }
            }
        }
    }

    pub(crate) fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        match m {
            filters_panel::Msg::Close => self.toggle_pane(PaneKind::Filters),
            _ => self.filters.update(m),
        }
    }

    pub(crate) fn fetch_mip_from_cache(&self, tab_id: TabId, mip: u32, range: TileRange) {
        let Some(tab) = self.state.tab(tab_id) else { return; };
        let (img_w, img_h) = (tab.desc.width, tab.desc.height);

        if let Ok(guard) = tab.viewport_cache.lock()
            && guard.has_all_tiles(mip, &range) {
                return;
            }

        crate::file_ops::fetch_mip(
            &tab.cache_dir,
            mip,
            range,
            img_w,
            img_h,
            tab.viewport_cache.clone(),
        );
    }

    pub(crate) fn push_error(&mut self, msg: String) {
        self.errors.push((msg, std::time::Instant::now()));
    }

    pub(crate) fn update_status_from_active_tab(&mut self) {
        if let Some(tab) = self.state.active_tab() {
            self.status.canvas_w = tab.desc.width;
            self.status.canvas_h = tab.desc.height;
            self.status.layers = tab.layers.len();
        } else {
            self.status.canvas_w = 0;
            self.status.canvas_h = 0;
            self.status.layers = 0;
        }
    }
}
