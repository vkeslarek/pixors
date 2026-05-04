use iced::widget::{container, text};
use iced::{Element, Length};

use crate::app::Msg;
use crate::theme::{TEXT_MUTED};

pub fn view<'a>() -> Element<'a, Msg> {
    container(
        text("Darkroom Page")
            .size(32)
            .color(TEXT_MUTED),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
