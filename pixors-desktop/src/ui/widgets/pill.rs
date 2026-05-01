use iced::border::Radius;
use iced::widget::{container, row, text};
use iced::{Background, Border, Element};

use crate::ui::theme::{self, BG_ELEVATED, BORDER, TEXT_SECONDARY};

pub fn pill<'a, Msg: 'a>(label: String) -> Element<'a, Msg> {
    let dot = container(text(""))
        .width(8)
        .height(8)
        .style(|_| container::Style {
            background: Some(Background::Color(theme::OK_GREEN)),
            border: Border::default().rounded(999),
            ..Default::default()
        });
    container(
        row![dot, text(label).size(10).color(TEXT_SECONDARY)]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .padding([4, 10])
    .style(|_| container::Style {
        background: Some(Background::Color(BG_ELEVATED)),
        border: Border {
            width: 1.0,
            color: BORDER,
            radius: Radius::from(20.0),
        },
        ..Default::default()
    })
    .into()
}
