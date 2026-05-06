use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use iced::keyboard::{self};
use iced::widget::pane_grid::{self, Configuration};
use iced::widget::{column, container, row};
use iced::{Background, Element, Length, Subscription};
use iced::futures::stream::StreamExt;
use pixors_executor::runtime::event::PipelineEvent;
use tokio::sync::broadcast;
use pixors_executor::source::cache_reader::TileRange;

use crate::components::{
    filters_panel, layers_panel, menu_bar, status_bar, tab_bar, toolbar,
    workspace_bar,
};
use crate::viewport::tile_cache::ViewportCache;

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
    Frames,
    PipelineEvent(PipelineEvent),
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
    pub cache: Option<Arc<Mutex<ViewportCache>>>,
    pub tile_generation: u64,
    /// Written by ViewportProgram when MIP changes; read here to trigger disk fetch.
    pub mip_fetch_signal: Arc<Mutex<Vec<(u32, TileRange)>>>,
    pub cache_dir: Option<PathBuf>,
    pub image_dims: Option<(u32, u32)>,
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
            cache: Some(ViewportCache::new()),
            tile_generation: 0,
            mip_fetch_signal: Arc::new(Mutex::new(Vec::new())),
            cache_dir: None,
            image_dims: None,
        }
    }
}

impl App {
    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![
            keyboard::listen().map(Msg::KeyPressed),
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Msg::Tick),
        ];

        let has_pending = self.cache.as_ref()
            .and_then(|c| c.lock().ok())
            .map(|g| g.has_pending())
            .unwrap_or(false);

        if self.loading || has_pending {
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
            crate::widgets::loading_bar(self.loading, self.progress),
            self.status.view::<Msg>(),
        ];

        let overlays = self.toasts_view();

        // Always return a stack at the root to avoid destroying the entire widget tree state
        // (including ViewportState) when toasts appear or disappear.
        iced::widget::stack![content, overlays].into()
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

