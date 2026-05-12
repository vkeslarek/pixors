use std::sync::{Arc, Mutex, RwLock};

use iced::widget::{column, container, shader as shader_widget, stack};
use iced::{Background, Color, Element, Length};

use pixors_engine::cache::cache_reader::TileRange;

use crate::components::pill::pill;
use crate::viewport::program::ViewportProgram;
use crate::viewport::tile_cache::TileCache;
use crate::viewport::viewport_state::ViewportState;
use pixors_document::SessionId;

pub fn view<'a, Msg: 'a>(
    tabs_view: Element<'a, Msg>,
    canvas_w: u32,
    canvas_h: u32,
    active_cache: Option<Arc<Mutex<TileCache>>>,
    redraw_seq: u64,
    mip_fetch_queue: Arc<Mutex<Vec<(SessionId, u32, TileRange)>>>,
    session_id: Option<SessionId>,
    viewport_state: Option<Arc<RwLock<ViewportState>>>,
) -> Element<'a, Msg> {
    let program = ViewportProgram {
        cache: active_cache,
        redraw_seq,
        mip_fetch_queue,
        session_id,
        viewport_state,
    };

    let canvas_bg = iced::widget::row![
        shader_widget(program)
            .width(Length::Fill)
            .height(Length::Fill),
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
                background: Some(Background::Color(Color::from_rgb(0.067, 0.067, 0.075,))),
                ..Default::default()
            }),
    ]
    .width(Length::Fill)
    .into()
}
