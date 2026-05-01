use iced::widget::{container, text};
use iced::{Element, Length};

use crate::ui::app::Msg;
use crate::ui::theme::{TEXT_MUTED};

pub fn view<'a>() -> Element<'a, Msg> {
    container(
        text("Library Page")
            .size(32)
            .color(TEXT_MUTED),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
