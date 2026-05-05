use std::sync::{Arc, Mutex};

use iced::widget::{column, container, shader as shader_widget, stack};
use iced::{Background, Color, Element, Length};

use pixors_executor::source::cache_reader::TileRange;

use crate::viewport::program::ViewportProgram;
use crate::viewport::tile_cache::ViewportCache;
use crate::widgets::pill;

pub fn view<'a, Msg: 'a>(
    tabs_view: Element<'a, Msg>,
    canvas_w: u32,
    canvas_h: u32,
    cache: Option<Arc<Mutex<ViewportCache>>>,
    tile_generation: u64,
    mip_fetch_signal: Arc<Mutex<Vec<(u32, TileRange)>>>,
) -> Element<'a, Msg> {
    let program = ViewportProgram { cache, tile_generation, mip_fetch_signal };

    let canvas_bg = shader_widget(program)
        .width(Length::Fill)
        .height(Length::Fill);

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
