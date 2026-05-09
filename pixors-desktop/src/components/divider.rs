use crate::theme::BORDER_SUBTLE;
use iced::widget::{container, text};
use iced::{Background, Element, Length};

pub fn divider<'a, Msg: 'a>() -> Element<'a, Msg> {
    container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(Background::Color(BORDER_SUBTLE)),
            ..Default::default()
        })
        .into()
}

pub fn v_divider<'a, Msg: 'a>() -> Element<'a, Msg> {
    container(text(""))
        .width(1)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BORDER_SUBTLE)),
            ..Default::default()
        })
        .into()
}
