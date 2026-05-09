use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::theme::{BG_SURFACE, BORDER_SUBTLE, TEXT_SECONDARY};
use crate::layout::panel;

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

        let preview_btn = crate::components::button("Cancel")
            .variant(crate::components::ButtonVariant::Danger)
            .size(crate::components::ButtonSize::Sm)
            .width(Length::Fill)
            .on_press(Msg::CancelPreview);

        column![label, slider, row![preview_btn].width(Length::Fill)]
            .spacing(10)
            .padding([12, 12])
            .into()
    }

    #[allow(dead_code)]
    pub fn view(&self) -> Element<'_, Msg> {
        let body = self.body_view();
        panel("Filters", body)
            .header_controls(
                crate::components::icon_button::icon_button(crate::icons::X).size(12).on_press(Msg::Close)
            )
            .into()
    }
}
