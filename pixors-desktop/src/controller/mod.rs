use std::sync::Arc;

use iced::widget::pane_grid;
use pixors_document::TabId;
use pixors_engine::runtime::event::PipelineEvent;

use crate::app::{App, Msg, PaneKind};
use crate::page::editor::tab_bar;
use crate::viewport::tile_cache_sink::unregister_tile_cache;

pub(crate) mod dialogs;
pub(crate) mod effects;
pub(crate) mod filters;
pub(crate) mod keyboard;
pub(crate) mod layers;
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
