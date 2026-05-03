use std::sync::{Arc, Mutex};

use iced::widget::{column, container, shader as shader_widget, stack};
use iced::{Background, Color, Element, Length};

use pixors_engine::pipeline::exec::display_sink::GpuBufferState;

use crate::engine::EngineProgram;
use crate::ui::widgets::pill;

pub fn view<'a, Msg: 'a>(
    tabs_view: Element<'a, Msg>,
    canvas_w: u32,
    canvas_h: u32,
    gpu_buffer: Option<&Arc<Mutex<GpuBufferState>>>,
) -> Element<'a, Msg> {
    let image = gpu_buffer
        .cloned()
        .unwrap_or_else(|| Arc::new(Mutex::new(GpuBufferState {
            pixels: vec![],
            width: 0,
            height: 0,
            dirty: false,
        })));

    let canvas_bg = shader_widget(EngineProgram { image })
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
