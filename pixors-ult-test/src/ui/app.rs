use iced::widget::pane_grid::{self, Configuration};
use iced::widget::{column, container, row};
use iced::{Background, Element, Length};

use crate::ui::components::{
    filters_panel, layers_panel, menu_bar, status_bar, tab_bar, toolbar,
    workspace_bar,
};
use crate::ui::theme::BG_SURFACE;

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
}

#[derive(Debug)]
pub struct App {
    pub panes: pane_grid::State<PaneKind>,
    pub workspace: workspace_bar::State,
    pub tools: toolbar::State,
    pub tabs: tab_bar::State,
    pub layers: layers_panel::State,
    pub filters: filters_panel::State,
    pub status: status_bar::State,
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
        }
    }
}

impl App {
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
            Msg::MenuBar(m) => match m {
                menu_bar::Msg::Exit => std::process::exit(0),
                menu_bar::Msg::ToggleLayers => self.toggle_pane(PaneKind::Layers),
                menu_bar::Msg::ToggleFilters => self.toggle_pane(PaneKind::Filters),
                menu_bar::Msg::ResetLayout => {
                    *self = Self::default();
                }
                _ => {}
            },
            Msg::WorkspaceBar(m) => self.workspace.update(m),
            Msg::Toolbar(m) => {
                self.tools.update(m);
                self.status.active_tool = self.tools.active_tool;
            }
            Msg::TabBar(m) => self.tabs.update(m),
            Msg::LayersPanel(m) => match m {
                layers_panel::Msg::Close => self.toggle_pane(PaneKind::Layers),
            },
            Msg::FiltersPanel(m) => match m {
                filters_panel::Msg::Close => self.toggle_pane(PaneKind::Filters),
                _ => self.filters.update(m),
            },
            Msg::PaneResized(e) => self.panes.resize(e.split, e.ratio),
            Msg::PaneDragged(e) => match e {
                pane_grid::DragEvent::Dropped { pane, target } => {
                    self.panes.drop(pane, target);
                }
                _ => {}
            },
            Msg::ClosePane(pane) => {
                let _ = self.panes.close(pane);
            }
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let active_page = match self.workspace.active {
            crate::ui::components::workspace_bar::Workspace::Editor => crate::ui::pages::editor::view(self),
            crate::ui::components::workspace_bar::Workspace::Library => crate::ui::pages::library::view(),
            crate::ui::components::workspace_bar::Workspace::Darkroom => crate::ui::pages::darkroom::view(),
        };

        column![
            menu_bar::view().map(Msg::MenuBar),
            row![
                self.workspace.view().map(Msg::WorkspaceBar),
                active_page,
            ]
            .height(Length::Fill),
            self.status.view(),
        ]
        .into()
    }


}
