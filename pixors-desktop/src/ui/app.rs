use std::sync::Arc;

use iced::keyboard::{self, Key};
use iced::widget::pane_grid::{self, Configuration};
use iced::widget::{column, container, row, text};
use iced::{Background, Element, Length, Subscription};

use crate::ui::components::{
    filters_panel, layers_panel, menu_bar, status_bar, tab_bar, toolbar,
    workspace_bar,
};
use crate::ui::components::toolbar::Tool;
use crate::viewport::program::PendingTileWrites;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Layers,
    Filters,
}

#[derive(Debug, Clone)]
pub enum Msg {
    MenuBar(menu_bar::Msg),
    WorkspaceBar(workspace_bar::Msg),
    Toolbar(toolbar::Msg),
    TabBar(tab_bar::Msg),
    LayersPanel(layers_panel::Msg),
    FiltersPanel(filters_panel::Msg),
    PaneResized(pane_grid::ResizeEvent),
    PaneDragged(pane_grid::DragEvent),
    ClosePane(pane_grid::Pane),
    KeyPressed(keyboard::Event),
    OpenFile,
    Tick,
}

pub struct App {
    pub panes: pane_grid::State<PaneKind>,
    pub workspace: workspace_bar::State,
    pub tools: toolbar::State,
    pub tabs: tab_bar::State,
    pub layers: layers_panel::State,
    pub filters: filters_panel::State,
    pub status: status_bar::State,
    pub loading: bool,
    pub progress: f32,
    pub progress_dir: f32,
    pub errors: Vec<(String, std::time::Instant)>,
    pub pending_writes: Arc<PendingTileWrites>,
    /// Incremented each tick while tiles are arriving. Forces Iced to
    /// re-evaluate the view so the shader's prepare() drains and uploads them.
    pub tile_generation: u64,
}

impl Default for App {
    fn default() -> Self {
        let cfg = Configuration::Split {
            axis: pane_grid::Axis::Horizontal,
            ratio: 0.55,
            a: Box::new(Configuration::Pane(PaneKind::Layers)),
            b: Box::new(Configuration::Pane(PaneKind::Filters)),
        };
        let panes = pane_grid::State::with_configuration(cfg);

        Self {
            panes,
            workspace: workspace_bar::State::default(),
            tools: toolbar::State::default(),
            tabs: tab_bar::State::default(),
            layers: layers_panel::State::default(),
            filters: filters_panel::State::default(),
            status: status_bar::State::default(),
            loading: true,
            progress: 0.0,
            progress_dir: 1.0,
            errors: Vec::new(),
            pending_writes: PendingTileWrites::new(),
            tile_generation: 0,
        }
    }
}

impl App {
    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::batch([
            keyboard::listen().map(Msg::KeyPressed),
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Msg::Tick),
        ])
    }

    fn find_pane(&self, kind: PaneKind) -> Option<pane_grid::Pane> {
        self.panes
            .iter()
            .find_map(|(p, k)| if *k == kind { Some(*p) } else { None })
    }

    fn restore_or_create(&mut self, kind: PaneKind) {
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

    fn toggle_pane(&mut self, kind: PaneKind) {
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

    fn handle_keyboard(&mut self, event: keyboard::Event) {
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

    fn open_file_dialog(&mut self) {
        self.loading = true;
        match crate::ui::file_ops::open_and_run(&self.pending_writes, None) {
            Ok((w, h, path)) => {
                self.status.canvas_w = w;
                self.status.canvas_h = h;
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
        self.loading = false;
    }

    fn handle_tick(&mut self) {
        if self.loading {
            self.progress += self.progress_dir * 0.02;
            if self.progress >= 1.0 {
                self.progress = 1.0;
                self.progress_dir = -1.0;
            } else if self.progress <= 0.0 {
                self.progress = 0.0;
                self.progress_dir = 1.0;
            }
            if self.progress >= 1.0 && self.progress_dir == -1.0 {
                self.loading = false;
            }
        }
        self.errors.retain(|(_, ts)| ts.elapsed().as_secs() < 5);

        // If tiles are queued by the background pipeline, bump tile_generation
        // so Iced sees a state change and re-renders, causing prepare() to drain
        // and upload the new tiles to the GPU texture.
        use std::sync::atomic::Ordering;
        if self.pending_writes.has_pending.load(Ordering::Relaxed) {
            self.tile_generation = self.tile_generation.wrapping_add(1);
        }
    }

    fn handle_menu_msg(&mut self, m: menu_bar::Msg) {
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

    fn handle_layers_msg(&mut self, m: layers_panel::Msg) {
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

    fn handle_filters_msg(&mut self, m: filters_panel::Msg) {
        match m {
            filters_panel::Msg::Close => self.toggle_pane(PaneKind::Filters),
            _ => self.filters.update(m),
        }
    }

    fn push_error(&mut self, msg: String) {
        self.errors.push((msg, std::time::Instant::now()));
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let active_page = match self.workspace.active {
            workspace_bar::Workspace::Editor => {
                crate::ui::pages::editor::view(self)
            }
            workspace_bar::Workspace::Library => crate::ui::pages::library::view(),
            workspace_bar::Workspace::Darkroom => crate::ui::pages::darkroom::view(),
        };

        let content = column![
            menu_bar::view().map(Msg::MenuBar),
            row![
                self.workspace.view().map(Msg::WorkspaceBar),
                active_page,
            ]
            .height(Length::Fill),
            self.loading_bar_view(),
            self.status.view::<Msg>(),
        ];

        let overlays = self.toasts_view();

        if !self.errors.is_empty() {
            iced::widget::stack![content, overlays].into()
        } else {
            content.into()
        }
    }

    fn loading_bar_view(&self) -> Element<'_, Msg> {
        if self.loading {
            let pct = self.progress.max(0.01) as u16;
            container(
                container(text(""))
                    .width(Length::FillPortion(pct))
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(crate::ui::theme::OK_GREEN)),
                        ..Default::default()
                    }),
            )
            .width(Length::Fill)
            .height(3)
            .style(|_| container::Style {
                background: Some(Background::Color(crate::ui::theme::BG_BASE)),
                ..Default::default()
            })
            .into()
        } else {
            container(text("")).height(0).into()
        }
    }

    fn toasts_view(&self) -> Element<'_, Msg> {
        if self.errors.is_empty() {
            return iced::widget::text("").into();
        }

        let mut toasts: Vec<Element<Msg>> = Vec::new();

        for (msg, _) in &self.errors {
            toasts.push(
                container(
                    row![
                        iced::widget::text("\u{26a0}")
                            .size(14)
                            .color(crate::ui::theme::ACCENT),
                        iced::widget::text(msg.as_str())
                            .size(11)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .padding([8, 12])
                .style(|_| container::Style {
                    background: Some(Background::Color(crate::ui::theme::BG_ELEVATED)),
                    border: iced::Border {
                        color: crate::ui::theme::ACCENT_DIM,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                })
                .into(),
            );
        }

        container(
            column(toasts)
                .spacing(8)
                .align_x(iced::Alignment::End),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([60, 20])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top)
        .into()
    }
}

