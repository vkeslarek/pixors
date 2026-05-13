pub mod tab_bar;
pub mod toolbar;
pub mod viewport;

use iced::widget::{column, container, pane_grid, row, stack, text};
use iced::{Alignment, Background, Color, Element, Length};

use crate::app::{App, Msg, PaneKind};
use crate::theme::{BG_SURFACE, TEXT_MUTED};

pub fn view<'a>(app: &'a App) -> Element<'a, Msg> {
    let active = app.state.active_session();
    let canvas_w = active.map(|t| t.document.canvas.width).unwrap_or(0);
    let canvas_h = active.map(|t| t.document.canvas.height).unwrap_or(0);
    let session_id = app.state.active_id();
    let active_cache = session_id
        .and_then(|id| app.viewport_tabs.get(&id))
        .map(|vt| vt.cache.clone());
    let viewport_state = session_id
        .and_then(|id| app.viewport_tabs.get(&id))
        .map(|vt| vt.state.clone());
    let redraw_seq = active.map(|t| t.transient.redraw_seq).unwrap_or(0);
    let mip_fetch_queue = session_id
        .and_then(|id| app.viewport_tabs.get(&id))
        .map(|vt| vt.mip_queue.clone())
        .unwrap_or_else(|| std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
    let loading = active.map(|t| t.transient.view.loading).unwrap_or(false);
    let progress = active.map(|t| t.transient.view.progress).unwrap_or(0.0);

    let canvas = if let Some(session_id) = session_id {
        let viewport = crate::page::editor::viewport::view(
            app.tabs.view(&app.state).map(Msg::TabBar),
            canvas_w,
            canvas_h,
            active_cache,
            redraw_seq,
            mip_fetch_queue,
            Some(session_id),
            viewport_state,
        );

        if loading {
            let pct = (progress.clamp(0.0, 1.0) * 100.0) as u8;
            let overlay = container(
                container(text(format!("Loading… {pct}%")))
                    .padding([2, 8])
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.7))),
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

    let grid: Element<'_, Msg> =
        crate::layout::pane_grid_layout::<PaneKind, Msg>(&app.panes, |pane, kind, _| {
            pane_content(app, pane, *kind)
        })
        .on_resize(Msg::PaneResized)
        .on_drag(Msg::PaneDragged)
        .width(app.sidebar_width)
        .into();

    let resizer = crate::components::resize_handle::resize_handle(Msg::SidebarResized);

    row![app.tools.view().map(Msg::Toolbar), canvas, resizer, grid,]
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
            // Iced needs the data to outlive the Element tree.
            // Use the tab's layers directly (they live in EditorState which is pinned).
            let layers = app
                .state
                .active_session()
                .map(|t| t.document.layers.as_slice())
                .unwrap_or(&[]);
            let active_id = app
                .state
                .active_session()
                .and_then(|t| t.transient.active_node);
            crate::panel::layers::view_slice(layers, active_id, &app.layers_panel)
                .map(Msg::LayersPanel)
        }
        PaneKind::Filters => {
            let transforms = app
                .state
                .active_session()
                .and_then(|t| {
                    t.transient
                        .active_node
                        .and_then(|id| t.document.layers.iter().find(|l| l.id == id))
                        .map(|l| l.transforms.as_slice())
                })
                .unwrap_or(&[]);
            crate::panel::filter::view(transforms, &app.filter_panel)
                .map(Msg::FiltersPanel)
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
