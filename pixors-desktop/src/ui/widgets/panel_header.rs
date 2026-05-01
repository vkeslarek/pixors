use iced::widget::{container, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::ui::theme::{BG_BASE, BORDER_SUBTLE, PANEL_HEADER_H, TEXT_MUTED, TEXT_SECONDARY};

pub fn panel_header<'a, Msg: 'a + Clone>(
    title: &'a str,
    on_close: Option<Msg>,
) -> Element<'a, Msg> {
    let drag = text(crate::ui::icons::GRIP_VERTICAL)
        .size(12)
        .font(crate::ui::icons::LUCIDE)
        .color(TEXT_MUTED);
    let title_el = text(title.to_uppercase())
        .size(11)
        .color(TEXT_SECONDARY);

    let mut r = row![drag, title_el].spacing(8).align_y(iced::Alignment::Center);

    if let Some(msg) = on_close {
        let close = iced::widget::button(
            text(crate::ui::icons::X)
                .size(12)
                .font(crate::ui::icons::LUCIDE)
                .color(TEXT_MUTED),
        )
            .on_press(msg)
            .padding(2)
            .style(|_, status| {
                let bg = if matches!(status, iced::widget::button::Status::Hovered) {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.08)
                } else {
                    Color::TRANSPARENT
                };
                iced::widget::button::Style {
                    background: Some(Background::Color(bg)),
                    border: Border::default().rounded(3),
                    text_color: TEXT_SECONDARY,
                    ..Default::default()
                }
            });
        r = r.push(iced::widget::Space::new().width(Length::Fill)).push(close);
    } else {
        r = r.push(iced::widget::Space::new().width(Length::Fill));
    }

    container(r.padding([0, 12]).align_y(iced::Alignment::Center))
        .width(Length::Fill)
        .height(PANEL_HEADER_H)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_BASE)),
            border: Border {
                width: 0.0,
                color: BORDER_SUBTLE,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
