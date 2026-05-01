use iced::widget::{container, text};
use iced::{Background, Element, Length};

use crate::ui::theme::BORDER_SUBTLE;

pub fn separator_h<'a, Msg: 'a>() -> Element<'a, Msg> {
    container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(Background::Color(BORDER_SUBTLE)),
            ..Default::default()
        })
        .into()
}
