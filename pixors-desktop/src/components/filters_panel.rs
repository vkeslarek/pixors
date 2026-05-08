use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::{BG_SURFACE, BORDER_SUBTLE, TEXT_SECONDARY};
use crate::widgets::panel_header;

#[derive(Debug, Clone)]
pub enum Msg {
    SetBlur(f32),
    CancelPreview,
    Close,
}

#[derive(Debug, Clone)]
pub struct State {
    pub blur_radius: f32,
    pub previewing: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            blur_radius: 3.0,
            previewing: false,
        }
    }
}

impl State {
    #[allow(dead_code)]
    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::SetBlur(v) => {
                self.blur_radius = v;
                self.previewing = true;
            }
            Msg::CancelPreview => {
                self.previewing = false;
            }
            Msg::Close => {}
        }
    }

    pub fn body_view(&self) -> Element<'_, Msg> {
        let label = text(format!("Gaussian Blur: {:.0}px", self.blur_radius))
            .size(11)
            .color(TEXT_SECONDARY);

        let slider = iced::widget::slider(1.0..=32.0, self.blur_radius, Msg::SetBlur)
            .width(Length::Fill)
            .step(1.0);

        let preview_btn = button(
            container(text("Cancel").size(12).color(Color::WHITE))
                .width(Length::Fill)
                .center_x(Length::Fill),
        )
        .on_press(Msg::CancelPreview)
        .padding([8, 0])
        .width(Length::Fill)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            let bg = if hovered {
                Color::from_rgb(0.80, 0.35, 0.35)
            } else {
                Color::from_rgb(0.70, 0.30, 0.30)
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border::default().rounded(5),
                text_color: Color::WHITE,
                ..Default::default()
            }
        });

        column![label, slider, row![preview_btn].width(Length::Fill)]
            .spacing(10)
            .padding([12, 12])
            .into()
    }

    #[allow(dead_code)]
    pub fn view(&self) -> Element<'_, Msg> {
        let header = panel_header::<Msg>("Filters", Some(Msg::Close));

        let body = self.body_view();

        container(column![header, body])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(BG_SURFACE)),
                border: Border {
                    width: 0.0,
                    color: BORDER_SUBTLE,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
