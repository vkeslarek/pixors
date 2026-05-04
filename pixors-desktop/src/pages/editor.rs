use iced::widget::{container, pane_grid, row};
use iced::{Background, Element, Length};

use crate::app::{App, Msg, PaneKind};
use crate::theme::BG_SURFACE;

pub fn view<'a>(app: &'a App) -> Element<'a, Msg> {
    row![
        app.tools.view().map(Msg::Toolbar),
        crate::components::viewport::view(
            app.tabs.view().map(Msg::TabBar),
            app.status.canvas_w,
            app.status.canvas_h,
            app.cache.clone(),
            app.tile_generation,
            app.mip_fetch_signal.clone(),
        ),
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
