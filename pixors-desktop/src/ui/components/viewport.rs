use std::sync::{Arc, Mutex};

use iced::widget::{column, container, shader as shader_widget, stack};
use iced::{Background, Color, Element, Length};

use crate::viewport::pipeline::ViewportPipeline;
use crate::viewport::program::{PendingTileWrites, ViewportProgram};
use crate::viewport::tiled_texture::TiledTexture;
use crate::ui::widgets::pill;

pub fn view<'a, Msg: 'a>(
    tabs_view: Element<'a, Msg>,
    canvas_w: u32,
    canvas_h: u32,
    pending_writes: Arc<PendingTileWrites>,
    tiled_texture: Option<Arc<Mutex<TiledTexture>>>,
) -> Element<'a, Msg> {
    let camera = Arc::new(Mutex::new(
        crate::viewport::camera::Camera::new(
            canvas_w.max(1) as f32,
            canvas_h.max(1) as f32,
        ),
    ));

    let program = ViewportProgram {
        tiled_texture,
        pending_writes,
        camera: camera.clone(),
    };

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
