use iced::widget::{column, container, pane_grid, row, text};
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

    let canvas = if let Some(tab_id) = tab_id {
        crate::components::viewport::view(
            app.tabs.view(&app.state).map(Msg::TabBar),
            canvas_w,
            canvas_h,
            active_cache,
            tile_generation,
            mip_fetch_signal,
            Some(tab_id),
            viewport_state,
        )
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

    row![
        app.tools.view().map(Msg::Toolbar),
        canvas,
        crate::components::sidebar_grid::view(
            &app.panes,
            |pane, kind| pane_content(app, pane, kind)
        ),
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
        PaneKind::Layers => app.layers.body_view().map(Msg::LayersPanel),
        PaneKind::Filters => app.filters.body_view().map(Msg::FiltersPanel),
    };
    let label = match kind {
        PaneKind::Layers => "LAYERS",
        PaneKind::Filters => "FILTERS",
    };

    let title_bar = crate::components::panel::title_bar(label, Msg::ClosePane(pane));

    let body = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_SURFACE)),
            ..Default::default()
        });

    pane_grid::Content::new(body).title_bar(title_bar)
}
