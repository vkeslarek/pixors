use iced::widget::tooltip::{Position};
use iced::widget::{container, text};
use iced::{Background, Border, Element};

pub fn tooltip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tip: impl Into<String>,
    position: Position,
) -> Element<'a, Message> {
    iced::widget::tooltip(
        content,
        container(text(tip.into()).size(10).color(crate::ui::theme::TEXT_PRIMARY))
            .padding([3, 6])
            .style(|_| container::Style {
                background: Some(Background::Color(crate::ui::theme::BG_ELEVATED)),
                border: Border {
                    radius: 4.0.into(),
                    color: crate::ui::theme::BORDER,
                    width: 1.0,
                },
                ..Default::default()
            }),
        position,
    )
    .gap(4)
    .into()
}
