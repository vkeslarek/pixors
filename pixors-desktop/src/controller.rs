use iced::keyboard::{self, Key};
use iced::widget::pane_grid;
use pixors_executor::runtime::event::PipelineEvent;
use pixors_executor::source::cache_reader::TileRange;

use crate::app::{App, Msg, PaneKind};
use crate::components::{filters_panel, layers_panel, menu_bar};
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
                    self.progress = if total > 0 {
                        done as f32 / total as f32
                    } else {
                        1.0
                    };
                    tracing::info!("[pixors] UI progress updated: {}/{} = {}", done, total, self.progress);
                }
                PipelineEvent::Done => {
                    self.progress = 1.0;
                    self.loading = false;
                    tracing::info!("[pixors] UI progress Done! set to 1.0");
                }
                PipelineEvent::Error(s) => {
                    self.push_error(s);
                    self.loading = false;
                }
            },
            Msg::MenuBar(m) => self.handle_menu_msg(m),
            Msg::WorkspaceBar(m) => self.workspace.update(m),
            Msg::Toolbar(m) => {
                self.tools.update(m);
                self.status.active_tool = self.tools.active_tool;
            }
            Msg::TabBar(m) => self.tabs.update(m),
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
                if let Key::Character("o") = key.as_ref() {
                    self.open_file_dialog();
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
        self.loading = true;
        self.progress = 0.0;
        tracing::info!("[pixors] open_file_dialog: reset progress to 0.0");
        match crate::file_ops::open_and_run(self.cache.clone()) {
            Ok((w, h, path)) => {
                self.status.canvas_w = w;
                self.status.canvas_h = h;
                self.cache_dir = Some(path.with_extension("pixors_cache"));
                self.image_dims = Some((w, h));
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

        if let Some(ref cache) = self.cache {
            if cache.lock().map_or(false, |g| g.has_pending()) {
                self.tile_generation = self.tile_generation.wrapping_add(1);
                tracing::info!("[pixors] handle_tick: tile_generation is now {}", self.tile_generation);
            }
        }

        let mut sigs = self.mip_fetch_signal.lock().unwrap();
        if !sigs.is_empty() {
            let reqs: Vec<_> = sigs.drain(..).collect();
            for (mip, range) in reqs {
                self.fetch_mip_from_cache(mip, range);
            }
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
            _ => {}
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

    pub(crate) fn fetch_mip_from_cache(&self, mip: u32, range: TileRange) {
        let Some(ref cache_dir) = self.cache_dir else { return; };
        let Some((img_w, img_h)) = self.image_dims else { return; };
        let Some(ref vp_cache) = self.cache else { return; };

        // Skip fetch if all visible tiles are already in RAM.
        if let Ok(guard) = vp_cache.lock() {
            if guard.has_all_tiles(mip, &range) {
                return;
            }
        }

        crate::file_ops::fetch_mip(
            cache_dir,
            mip,
            range,
            img_w,
            img_h,
            vp_cache.clone(),
        );
    }

    pub(crate) fn push_error(&mut self, msg: String) {
        self.errors.push((msg, std::time::Instant::now()));
    }
}
