use iced::widget::pane_grid;

use crate::app::{App, Msg, PaneKind};

pub(crate) mod dialogs;
pub(crate) mod effects;
pub(crate) mod filters;
pub(crate) mod keyboard;
pub(crate) mod layers;
pub(crate) mod pipeline;
pub(crate) mod tabs;
pub(crate) mod tick;
pub(crate) mod viewport;

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
            Msg::PipelineEvent(e) => self.handle_pipeline_event(e),
            Msg::PipelineLagged(skipped) => {
                tracing::warn!(
                    "pipeline event channel lagged, skipped={skipped}; resyncing tab locks"
                );
                self.dispatcher.resync_locks(&mut self.state);
            }
            Msg::ExportDialog(m) => self.handle_export_dialog(m),
            Msg::UiShowcase(m) => self.handle_ui_showcase(m),
            Msg::FilterSearch(m) => self.handle_filter_search(m),
            Msg::MenuBar(m) => self.handle_menu_msg(m),
            Msg::WorkspaceBar(m) => self.workspace.update(m),
            Msg::Toolbar(m) => {
                self.tools.update(m);
                self.status.active_tool = self.tools.active_tool;
            }
            Msg::TabBar(m) => self.handle_tab_bar(m),
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

    pub(crate) fn push_error(&mut self, msg: String) {
        self.errors.push((msg, std::time::Instant::now()));
    }

    pub(crate) fn update_status_from_active_tab(&mut self) {
        if let Some(tab) = self.state.active_session() {
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
