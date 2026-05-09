use std::sync::{Arc, OnceLock};

use iced::keyboard::{self};
use iced::widget::pane_grid::{self, Configuration};
use iced::widget::{column, container, row, text};
use iced::{Background, Color, Element, Length, Subscription};
use pixors_engine::runtime::event::PipelineEvent;
use tokio::sync::broadcast;

use pixors_state::action::{Action, Dispatcher};
use crate::page::{
    menu_bar, status_bar, workspace_bar,
    editor::{tab_bar, toolbar},
};
use crate::panel::{filter as filters_panel, layers as layers_panel};
use pixors_state::state::EditorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Layers,
    Filters,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Msg {
    Action(Arc<dyn Action>),
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
    Frames,
    PipelineEvent(PipelineEvent),
    PipelineLagged(u64),
    ExportDialog(crate::modal::export::Msg),
    UiShowcase(crate::modal::ui_showcase::Msg),
}

pub struct App {
    pub dispatcher: Dispatcher,
    pub state: EditorState,
    pub panes: pane_grid::State<PaneKind>,
    pub workspace: workspace_bar::State,
    pub tools: toolbar::State,
    pub tabs: tab_bar::State,
    pub layers: layers_panel::State,
    pub filters: filters_panel::State,
    pub status: status_bar::State,
    #[allow(dead_code)]
    pub errors: Vec<(String, std::time::Instant)>,
    pub show_export_dialog: bool,
    pub export_dialog: crate::modal::export::ExportDialog,
    pub show_ui_showcase: bool,
    pub ui_showcase: crate::modal::ui_showcase::UiShowcase,
}

static PIPELINE_BROADCAST: OnceLock<broadcast::Sender<PipelineEvent>> = OnceLock::new();

pub fn pipeline_event_tx() -> broadcast::Sender<PipelineEvent> {
    PIPELINE_BROADCAST
        .get_or_init(|| broadcast::channel(256).0)
        .clone()
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

        let state = EditorState::new();

        let mut app = Self {
            dispatcher: Dispatcher::new(pipeline_event_tx()),
            state,
            panes,
            workspace: workspace_bar::State::default(),
            tools: toolbar::State::default(),
            tabs: tab_bar::State::default(),
            layers: layers_panel::State::default(),
            filters: filters_panel::State::default(),
            status: status_bar::State::default(),
            errors: Vec::new(),
            show_export_dialog: false,
            export_dialog: crate::modal::export::ExportDialog::default(),
            show_ui_showcase: false,
            ui_showcase: crate::modal::ui_showcase::UiShowcase::default(),
        };
        app.update_status_from_active_tab();
        app
    }
}

impl App {
    pub fn active_file_path(&self) -> Option<&std::path::Path> {
        self.state.active_tab().and_then(|t| match &t.source {
            pixors_state::state::TabSource::File { path } => Some(path.as_path()),
            pixors_state::state::TabSource::NewBlank { .. } => None,
        })
    }

    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![
            keyboard::listen().map(Msg::KeyPressed),
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Msg::Tick),
            iced::window::frames().map(|_| Msg::Frames),
        ];

        subs.push(Self::pipeline_subscription());

        Subscription::batch(subs)
    }

    fn pipeline_subscription() -> Subscription<Msg> {
        Subscription::run_with("pipeline_progress", |_id| {
            let rx = pipeline_event_tx().subscribe();
            iced::futures::stream::unfold(rx, |mut rx| async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => return Some((Msg::PipelineEvent(event), rx)),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                "pipeline event channel lagged, skipped={skipped}; resyncing tab locks"
                            );
                            return Some((Msg::PipelineLagged(skipped), rx));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                    }
                }
            })
        })
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let active_page = match self.workspace.active {
            workspace_bar::Workspace::Editor => crate::page::editor::view(self),
            workspace_bar::Workspace::Library => crate::page::library::view(),
            workspace_bar::Workspace::Darkroom => crate::page::darkroom::view(),
        };

        let content = column![
            menu_bar::view().map(Msg::MenuBar),
            row![self.workspace.view().map(Msg::WorkspaceBar), active_page,].height(Length::Fill),
            self.status.view::<Msg>(),
        ];

        let overlays = self.overlays_view();

        // Always return a stack at the root to avoid destroying the entire widget tree state
        // (including ViewportState) when toasts appear or disappear.
        iced::widget::stack![content, overlays].into()
    }

    fn overlays_view(&self) -> Element<'_, Msg> {
        let mut layers: Vec<Element<Msg>> = Vec::new();

        if self.show_export_dialog {
            let backdrop = container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
                    ..Default::default()
                });
            layers.push(
                iced::widget::stack![backdrop, self.export_dialog.view().map(Msg::ExportDialog),]
                    .into(),
            );
        }

        if self.show_ui_showcase {
            let backdrop = container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.6))),
                    ..Default::default()
                });
            layers.push(
                iced::widget::stack![
                    backdrop,
                    container(self.ui_showcase.view().map(Msg::UiShowcase))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                ]
                .into(),
            );
        }

        layers.push(self.toasts_view());

        iced::widget::stack(layers).into()
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
                            .color(crate::theme::ACCENT),
                        iced::widget::text(msg.as_str())
                            .size(11)
                            .color(crate::theme::TEXT_SECONDARY),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .padding([8, 12])
                .style(|_| container::Style {
                    background: Some(Background::Color(crate::theme::BG_ELEVATED)),
                    border: iced::Border {
                        color: crate::theme::ACCENT_DIM,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                })
                .into(),
            );
        }

        container(column(toasts).spacing(8).align_x(iced::Alignment::End))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([60, 20])
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Top)
            .into()
    }
}
