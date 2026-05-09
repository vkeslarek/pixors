use iced::widget::{column, container, pane_grid, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};
use crate::theme::{ACCENT, BG_BASE, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};
use crate::layout::ghost_width::GhostWidth;

pub struct Panel<'a, Message> {
    title: String,
    header_controls: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
}

pub fn panel<'a, Message>(
    title: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
) -> Panel<'a, Message> {
    Panel {
        title: title.into(),
        header_controls: None,
        content: content.into(),
        footer: None,
    }
}

impl<'a, Message> Panel<'a, Message> {
    pub fn header_controls(mut self, controls: impl Into<Element<'a, Message>>) -> Self {
        self.header_controls = Some(controls.into());
        self
    }

    pub fn footer(mut self, footer: impl Into<Element<'a, Message>>) -> Self {
        self.footer = Some(footer.into());
        self
    }
}

impl<'a, Message: 'a> From<Panel<'a, Message>> for Element<'a, Message> {
    fn from(p: Panel<'a, Message>) -> Self {
        let mut header_row = row![
            text(p.title)
                .size(13)
                .color(TEXT_PRIMARY)
        ]
        .align_y(Alignment::Center);

        if let Some(controls) = p.header_controls {
            header_row = header_row.push(iced::widget::Space::new().width(Length::Fill));
            header_row = header_row.push(controls);
        }

        let header = container(header_row)
            .padding(Padding::from([12, 16]))
            .width(Length::Fill)
            .style(|_| container::Style {
                border: Border {
                    width: 1.0,
                    color: BORDER_SUBTLE,
                    ..Border::default()
                },
                ..Default::default()
            });

        let mut col = column![
            header,
            scrollable(
                container(p.content)
                    .width(Length::Fill)
            )
            .height(Length::Fill)
            .width(Length::Fill)
        ];

        if let Some(footer) = p.footer {
            col = col.push(
                container(footer)
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        border: Border {
                            width: 1.0,
                            color: BORDER_SUBTLE,
                            ..Border::default()
                        },
                        ..Default::default()
                    })
            );
        }

        container(col)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(BG_SURFACE)),
                ..Default::default()
            })
            .into()
    }
}

pub fn title_bar<'a, Message: Clone + 'a>(
    title: impl Into<String>,
    on_close: Option<Message>,
) -> pane_grid::TitleBar<'a, Message> {
    let header = container(
        row![
            text(crate::icons::GRIP_VERTICAL)
                .size(12)
                .font(crate::icons::LUCIDE)
                .color(TEXT_MUTED),
            text(title.into()).size(11).color(ACCENT),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .padding([4, 12]),
    )
    .align_y(iced::alignment::Vertical::Center);

    let header = GhostWidth::new(header);

    let mut controls_row = row![].align_y(iced::Alignment::Center);

    if let Some(msg) = on_close {
        let close = iced::widget::button(
            text(crate::icons::X)
                .size(12)
                .font(crate::icons::LUCIDE)
                .color(TEXT_MUTED),
        )
        .on_press(msg)
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
        controls_row = controls_row.push(close);
    }

    let controls = container(controls_row)
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
