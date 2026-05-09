pub mod tab_bar;
pub mod toolbar;
pub mod viewport;

use iced::widget::{column, container, pane_grid, row, stack, text};
use iced::{Alignment, Background, Color, Element, Length};

use crate::app::{App, Msg, PaneKind};
use crate::theme::{BG_SURFACE, TEXT_MUTED};

pub fn view<'a>(app: &'a App) -> Element<'a, Msg> {
    let active = app.state.active_tab();
    let canvas_w = active.map(|t| t.desc.width).unwrap_or(0);
    let canvas_h = active.map(|t| t.desc.height).unwrap_or(0);
    let active_cache = active.map(|t| t.viewport_cache.clone());
    let tab_id = app.state.active_id();
    let viewport_state = active.map(|t| t.viewport_state.clone());
    let tile_generation = active.map(|t| t.tile_generation).unwrap_or(0);
    let mip_fetch_signal = active
        .map(|t| t.mip_fetch_signal.clone())
        .unwrap_or_else(|| std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
    let loading = active.map(|t| t.view.loading).unwrap_or(false);
    let progress = active.map(|t| t.view.progress).unwrap_or(0.0);

    let canvas = if let Some(tab_id) = tab_id {
        let viewport = crate::page::editor::viewport::view(
            app.tabs.view(&app.state).map(Msg::TabBar),
            canvas_w,
            canvas_h,
            active_cache,
            tile_generation,
            mip_fetch_signal,
            Some(tab_id),
            viewport_state,
        );

        if loading {
            let pct = (progress.clamp(0.0, 1.0) * 100.0) as u8;
            let overlay = container(
                container(text(format!("Loading… {pct}%")))
                    .padding([2, 8])
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.0, 0.0, 0.0, 0.7,
                        ))),
                        ..Default::default()
                    }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
            stack![viewport, overlay].into()
        } else {
            viewport
        }
    } else {
        column![
            app.tabs.view(&app.state).map(Msg::TabBar),
            container(
                text("Open an image to start\n(Ctrl+O)")
                    .size(14)
                    .color(TEXT_MUTED),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(
                    0.067, 0.067, 0.075, 1.0,
                ))),
                ..Default::default()
            }),
        ]
        .width(Length::Fill)
        .into()
    };

    let grid: Element<'_, Msg> = crate::layout::pane_grid_layout::<PaneKind, Msg>(&app.panes, |pane, kind, _| pane_content(
        app, pane, *kind
    ))
    .on_resize(|e| Msg::PaneResized(e))
    .on_drag(|e| Msg::PaneDragged(e))
    .width(crate::theme::SIDEBAR_W)
    .into();

    row![
        app.tools.view().map(Msg::Toolbar),
        canvas,
        grid,
    ]
    .height(Length::Fill)
    .into()
}

fn pane_content<'a>(
    app: &'a App,
    pane: pane_grid::Pane,
    kind: PaneKind,
) -> pane_grid::Content<'a, Msg> {
    let body: Element<Msg> = match kind {
        PaneKind::Layers => {
            let idx = app.state.active_tab()
                .and_then(|t| t.active_layer)
                .and_then(|active_id| {
                    app.state.active_tab()?.layers.iter().position(|l| l.id == active_id)
                })
                .unwrap_or(0);
            let layers = app.state.active_tab()
                .map(|t| t.layers.as_slice())
                .unwrap_or(&[]);
            crate::panel::layers::view(layers, idx).map(Msg::LayersPanel)
        }
        PaneKind::Filters => {
            let radius = app.state.active_tab()
                .map(|t| t.filter.blur_radius)
                .unwrap_or(3.0);
            crate::panel::filter::body_view(radius).map(Msg::FiltersPanel)
        }
    };
    let label = match kind {
        PaneKind::Layers => "LAYERS",
        PaneKind::Filters => "FILTERS",
    };

    let title_bar = crate::layout::pane_title_bar(label, Some(Msg::ClosePane(pane)));

    let body = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_SURFACE)),
            ..Default::default()
        });

    pane_grid::Content::new(body).title_bar(title_bar)
}
