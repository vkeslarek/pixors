use std::path::PathBuf;
use std::sync::OnceLock;

use iced::keyboard::{self};
use iced::widget::pane_grid::{self, Configuration};
use iced::widget::{column, container, row, text};
use iced::{Background, Color, Element, Length, Subscription};
use pixors_executor::runtime::event::PipelineEvent;
use tokio::sync::broadcast;

use crate::components::{
    filters_panel, layers_panel, menu_bar, status_bar, tab_bar, toolbar,
    workspace_bar,
};
use crate::state::EditorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Layers,
    Filters,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    Frames,
    PipelineEvent(PipelineEvent),
    ExportDialog(crate::dialog::export::Msg),
}

pub struct App {
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
    pub image_path: Option<PathBuf>,
    pub show_export_dialog: bool,
    pub export_dialog: crate::dialog::export::ExportDialog,
}

static PIPELINE_BROADCAST: OnceLock<broadcast::Sender<PipelineEvent>> = OnceLock::new();

pub fn pipeline_event_tx() -> broadcast::Sender<PipelineEvent> {
    PIPELINE_BROADCAST
        .get_or_init(|| broadcast::channel(64).0)
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
            state,
            panes,
            workspace: workspace_bar::State::default(),
            tools: toolbar::State::default(),
            tabs: tab_bar::State::default(),
            layers: layers_panel::State::default(),
            filters: filters_panel::State::default(),
            status: status_bar::State::default(),
            errors: Vec::new(),
            image_path: None,
            show_export_dialog: false,
            export_dialog: crate::dialog::export::ExportDialog::default(),
        };
        app.update_status_from_active_tab();
        app
    }
}

impl App {
    pub fn loading_active(&self) -> bool {
        self.state.active_tab().map(|t| t.view.loading).unwrap_or(false)
    }

    pub fn progress_active(&self) -> f32 {
        self.state.active_tab().map(|t| t.view.progress).unwrap_or(0.0)
    }

    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![
            keyboard::listen().map(Msg::KeyPressed),
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Msg::Tick),
        ];

        let has_pending = self.state.active_tab()
            .and_then(|t| t.viewport_cache.lock().ok())
            .is_some_and(|g| g.has_pending());

        let tab_loading = self.loading_active();

        if tab_loading || has_pending {
            subs.push(iced::window::frames().map(|_| Msg::Frames));
        }

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
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                    }
                }
            })
        })
    }


    pub fn view(&self) -> Element<'_, Msg> {
        let active_page = match self.workspace.active {
            workspace_bar::Workspace::Editor => {
                crate::pages::editor::view(self)
            }
            workspace_bar::Workspace::Library => crate::pages::library::view(),
            workspace_bar::Workspace::Darkroom => crate::pages::darkroom::view(),
        };

        let content = column![
            menu_bar::view().map(Msg::MenuBar),
            row![
                self.workspace.view().map(Msg::WorkspaceBar),
                active_page,
            ]
            .height(Length::Fill),
            crate::widgets::loading_bar(self.loading_active(), self.progress_active()),
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
                    background: Some(Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.6,
                    ))),
                    ..Default::default()
                });
            layers.push(
                iced::widget::stack![
                    backdrop,
                    self.export_dialog.view().map(Msg::ExportDialog),
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

