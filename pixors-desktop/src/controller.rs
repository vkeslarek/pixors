use pixors_state::state::TabId;
use std::sync::Arc;

use iced::keyboard::{self, Key};
use iced::widget::pane_grid;
use pixors_engine::runtime::event::PipelineEvent;
use pixors_ops::source::cache_reader::TileRange;

use crate::app::{App, Msg, PaneKind};
use crate::page::editor::toolbar::Tool;
use crate::page::menu_bar;
use crate::page::editor::tab_bar;
use crate::panel::{filter as filters_panel, layers as layers_panel};

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
            Msg::Frames => {} // Just to wake up the event loop
            Msg::PipelineEvent(e) => match e {
                PipelineEvent::Progress { tag, done, total } => {
                    let p = if total > 0 {
                        done as f32 / total as f32
                    } else {
                        1.0
                    };
                    let tab_id = TabId(tag);
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.view.progress = p;
                    }
                }
                PipelineEvent::Done { tag } => {
                    let tab_id = TabId(tag);
                    self.dispatcher.on_pipeline_done(&mut self.state, tab_id);
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.view.loading = false;
                        tab.view.progress = 1.0;
                    }
                }
                PipelineEvent::Error { tag, message } => {
                    let tab_id = TabId(tag);
                    self.dispatcher
                        .on_pipeline_error(&mut self.state, tab_id, message.clone());
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.view.loading = false;
                    }
                    self.push_error(message);
                }
                PipelineEvent::Cancelled { tag } => {
                    let tab_id = TabId(tag);
                    if let Some(tab) = self.state.tab_mut(tab_id) {
                        tab.view.loading = false;
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
            Msg::MenuBar(m) => self.handle_menu_msg(m),
            Msg::WorkspaceBar(m) => self.workspace.update(m),
            Msg::Toolbar(m) => {
                self.tools.update(m);
                self.status.active_tool = self.tools.active_tool;
            }
            Msg::TabBar(m) => match m {
                tab_bar::Msg::Select(id) => {
                    if let Err(e) = self.dispatcher.dispatch(
                        Arc::new(pixors_state::action::actions::switch_tab::SwitchTab(id)),
                        &mut self.state,
                    ) {
                        self.push_error(e);
                    }
                    self.update_status_from_active_tab();
                }
                tab_bar::Msg::Close(id) => {
                    if let Err(e) = self.dispatcher.dispatch(
                        Arc::new(pixors_state::action::actions::close_tab::CloseTab(id)),
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
                    Key::Character("e") => {
                        if self.active_file_path().is_some() {
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
        let path = rfd::FileDialog::new()
            .add_filter("Images", &["png", "tiff", "tif"])
            .pick_file();

        if let Some(path) = path {
            if let Err(e) = self.dispatcher.dispatch(
                Arc::new(pixors_state::action::actions::open_file::OpenFile::new(
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

    pub(crate) fn handle_tick(&mut self) {
        self.errors.retain(|(_, ts)| ts.elapsed().as_secs() < 5);

        let mut mip_requests: Vec<(TabId, u32, TileRange, std::path::PathBuf, u32, u32)> =
            Vec::new();

        for tab in &mut self.state.tabs {
            if tab.viewport_cache.lock().is_ok_and(|g| g.has_pending()) {
                tab.tile_generation = tab.tile_generation.wrapping_add(1);
            }

            let mut sigs = tab.mip_fetch_signal.lock().unwrap();
            if !sigs.is_empty() {
                for (tab_id, mip, range) in sigs.drain(..) {
                    let cache_dir = tab.cache_dir.clone();
                    let (img_w, img_h) = (tab.desc.width, tab.desc.height);
                    mip_requests.push((tab_id, mip, range, cache_dir, img_w, img_h));
                }
            }
        }

        for (tab_id, mip, range, cache_dir, img_w, img_h) in mip_requests {
            if let Some(tab) = self.state.tab(tab_id)
                && let Ok(guard) = tab.viewport_cache.lock()
                && guard.has_all_tiles(mip, &range)
            {
                continue;
            }

            let _ = self.dispatcher.dispatch(
                Arc::new(pixors_state::action::actions::mip_fetch::RequestMipFetch {
                    tab: tab_id,
                    mip,
                    range,
                    cache_dir,
                    img_w,
                    img_h,
                    post_process: None,
                }),
                &mut self.state,
            );
        }
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
            menu_bar::Msg::Export => {
                if self.active_file_path().is_some() {
                    self.show_export_dialog = true;
                }
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
                        tab.view.loading = true;
                        tab.view.progress = 0.0;

                        let action = Arc::new(pixors_state::action::actions::export::Export {
                            tab: tab_id,
                            source_path: path.clone(),
                            save_path: save_path.clone(),
                            config: config.clone(),
                            dpi: tab.desc.dpi,
                            icc_profile: tab.desc.icc_profile.clone(),
                            image_height: tab.desc.height,
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
            layers_panel::Msg::Select(i) => {
                if let Some(tab) = self.state.active_tab_mut() {
                    if let Some(layer) = tab.layers.get(i) {
                        tab.active_layer = Some(layer.id);
                    }
                }
            }
            layers_panel::Msg::ToggleVisibility(i) => {
                if let Some(tab) = self.state.active_tab_mut() {
                    if let Some(layer) = tab.layers.get_mut(i) {
                        layer.visible = !layer.visible;
                    }
                }
            }
        }
    }

    pub(crate) fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        match m {
            filters_panel::Msg::Close => self.toggle_pane(PaneKind::Filters),
            filters_panel::Msg::SetBlur(v) => {
                if let Some(tab) = self.state.active_tab_mut() {
                    if tab.filter.blur_radius != v {
                        tab.filter.blur_radius = v;
                        drop(tab);
                        self.dispatch_blur_preview(v as u32);
                    }
                }
            }
            filters_panel::Msg::CancelPreview => {
                self.dispatch_blur_cancel();
            }
        }
    }

    fn dispatch_blur_preview(&mut self, radius: u32) {
        let (
            tab_id,
            mip,
            img_w,
            img_h,
            cache,
            generation,
            display_format,
            display_color_space,
            working_format,
            working_color_space,
        ) = {
            let Some(tab) = self.state.active_tab() else {
                return;
            };
            (
                tab.id,
                tab.viewport_state.read().unwrap().current_mip,
                tab.desc.width,
                tab.desc.height,
                tab.viewport_cache.clone(),
                tab.tile_generation.wrapping_add(1),
                self.state.display_format,
                self.state.display_color_space,
                self.state.working_format,
                self.state.working_color_space,
            )
        };

        let action = Arc::new(pixors_state::action::actions::blur_preview::BlurPreview {
            tab: tab_id,
            radius,
            generation,
            mip,
            image_width: img_w,
            image_height: img_h,
            cache,
            display_format,
            display_color_space,
            working_format,
            working_color_space,
        });

        if let Err(e) = self.dispatcher.dispatch(action, &mut self.state) {
            self.push_error(e);
        }
    }

    fn dispatch_blur_cancel(&mut self) {
        let (tab_id, generation, mip, range, signal) = {
            let Some(tab) = self.state.active_tab() else {
                return;
            };
            (
                tab.id,
                tab.tile_generation.wrapping_add(1),
                tab.viewport_state.read().unwrap().current_mip,
                tab.viewport_state.read().unwrap().camera.padded_tile_range(
                    tab.viewport_state.read().unwrap().current_mip,
                    256,
                    1,
                ),
                tab.mip_fetch_signal.clone(),
            )
        };

        let action = Arc::new(pixors_state::action::actions::blur_cancel::BlurCancel {
            tab: tab_id,
            generation,
        });

        if let Err(e) = self.dispatcher.dispatch(action, &mut self.state) {
            self.push_error(e);
        }

        if let Ok(mut sigs) = signal.lock() {
            sigs.push((tab_id, mip, range));
        }
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
