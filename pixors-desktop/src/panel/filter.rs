use iced::widget::{column, row, text};
use iced::{Element, Length};

use crate::layout::panel;
use crate::theme::TEXT_SECONDARY;

#[derive(Debug, Clone)]
pub enum Msg {
    SetBlur(f32),
    CommitBlur(f32),
    CancelPreview,
    Close,
    OpenFilterSearch,
}

pub fn body_view<'a>(blur_radius: f32) -> Element<'a, Msg> {
    let label = text(format!("Gaussian Blur: {:.0}px", blur_radius))
        .size(11)
        .color(TEXT_SECONDARY);

    let slider = iced::widget::slider(1.0..=32.0, blur_radius, Msg::SetBlur)
        .width(Length::Fill)
        .step(1.0)
        .on_release(Msg::CommitBlur(blur_radius));

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

pub fn view<'a>(blur_radius: f32) -> Element<'a, Msg> {
    let body = body_view(blur_radius);
    panel("Filters", body)
        .header_controls(
            crate::components::icon_button::icon_button(crate::icons::X)
                .size(12)
                .on_press(Msg::Close),
        )
        .into()
}
