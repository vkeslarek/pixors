use std::sync::{Arc, Mutex};

use iced::widget::{column, container, shader as shader_widget, stack};
use iced::{Background, Color, Element, Length};

use pixors_executor::source::cache_reader::TileRange;

use crate::state::TabId;
use crate::viewport::program::ViewportProgram;
use crate::viewport::tile_cache::ViewportCache;
use crate::widgets::pill;

pub fn view<'a, Msg: 'a>(
    tabs_view: Element<'a, Msg>,
    canvas_w: u32,
    canvas_h: u32,
    active_cache: Option<Arc<Mutex<ViewportCache>>>,
    tile_generation: u64,
    mip_fetch_signal: Arc<Mutex<Vec<(TabId, u32, TileRange)>>>,
    tab_id: Option<TabId>,
) -> Element<'a, Msg> {
    let program = ViewportProgram {
        cache: active_cache,
        tile_generation,
        mip_fetch_signal,
        tab_id,
    };

    // By alternating the width of a sibling space widget slightly, we force
    // iced to invalidate the shader widget's bounds cache and call draw(),
    // allowing new tiles to be displayed immediately without waiting for events.
    let pad = if tile_generation.is_multiple_of(2) { 0.0 } else { 1.0 };
    let canvas_bg = iced::widget::row![
        shader_widget(program)
            .width(Length::Fill)
            .height(Length::Fill),
        iced::widget::Space::new().width(pad)
    ];

    let overlay = container(pill(format!("{}×{}", canvas_w, canvas_h)))
        .padding(12)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Bottom)
        .width(Length::Fill)
        .height(Length::Fill);

    let canvas = stack![canvas_bg, overlay];

    column![
        tabs_view,
        container(canvas)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb(
                    0.067, 0.067, 0.075,
                ))),
                ..Default::default()
            }),
    ]
    .width(Length::Fill)
    .into()
}
