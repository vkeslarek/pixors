use iced::widget::{container, pane_grid, row};
use iced::{Background, Border, Color, Length};

use crate::ui::theme::{ACCENT, BG_BASE, BORDER_SUBTLE, TEXT_MUTED, TEXT_SECONDARY};

pub fn title_bar<'a, Msg: Clone + 'a>(
    title: &'a str,
    on_close: Msg,
) -> pane_grid::TitleBar<'a, Msg> {
    let header = container(
        row![
            iced::widget::text(crate::ui::icons::GRIP_VERTICAL)
                .size(12)
                .font(crate::ui::icons::LUCIDE)
                .color(TEXT_MUTED),
            iced::widget::text(title).size(11).color(ACCENT),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .padding([4, 12]),
    )
    .align_y(iced::alignment::Vertical::Center);

    let header = crate::ui::widgets::ghost_width::GhostWidth::new(header);

    let close = iced::widget::button(
        iced::widget::text(crate::ui::icons::X)
            .size(12)
            .font(crate::ui::icons::LUCIDE)
            .color(TEXT_MUTED),
    )
    .on_press(on_close)
    .padding(4)
    .style(|_, status| {
        let hovered = matches!(status, iced::widget::button::Status::Hovered);
        let bg = if hovered {
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

    let controls = container(close)
        .width(Length::Shrink)
        .padding(iced::Padding::new(0.0).right(8))
        .align_y(iced::alignment::Vertical::Center);

    pane_grid::TitleBar::new(header)
        .controls(pane_grid::Controls::new(controls))
        .always_show_controls()
        .padding([4, 8])
        .style(|_: &iced::Theme| container::Style {
            background: Some(Background::Color(BG_BASE)),
            border: Border {
                width: 0.0,
                color: BORDER_SUBTLE,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
}
