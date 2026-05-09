use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Background, Border, Element, Length, Padding};
use crate::theme::{BG_SURFACE, BORDER_SUBTLE, TEXT_PRIMARY};
use crate::components::button::{button, ButtonVariant, ButtonSize};

pub struct Dialog<'a, Message> {
    title: String,
    content: Element<'a, Message>,
    on_close: Option<Message>,
    width: f32,
    height: f32,
}

pub fn dialog<'a, Message>(
    title: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
) -> Dialog<'a, Message> {
    Dialog {
        title: title.into(),
        content: content.into(),
        on_close: None,
        width: 600.0,
        height: 500.0,
    }
}

impl<'a, Message> Dialog<'a, Message> {
    pub fn on_close(mut self, msg: Message) -> Self {
        self.on_close = Some(msg);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

impl<'a, Message: Clone + 'a> From<Dialog<'a, Message>> for Element<'a, Message> {
    fn from(d: Dialog<'a, Message>) -> Self {
        let mut title_row = row![
            text(d.title)
                .size(16)
                .color(TEXT_PRIMARY),
            Space::new().width(Length::Fill),
        ]
        .align_y(Alignment::Center)
        .padding(Padding::from([16, 20]))
        .width(Length::Fill);

        if let Some(msg) = d.on_close {
            title_row = title_row.push(
                button("Close")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .on_press(msg)
            );
        }

        let divider = container(text(""))
            .width(Length::Fill)
            .height(1)
            .style(|_| container::Style {
                background: Some(Background::Color(BORDER_SUBTLE)),
                ..Default::default()
            });

        container(
            column![
                title_row,
                divider,
                d.content,
            ]
        )
        .width(Length::Fixed(d.width))
        .height(Length::Fixed(d.height))
        .style(|_| container::Style {
            background: Some(Background::Color(BG_SURFACE)),
            border: Border {
                color: BORDER_SUBTLE,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
    }
}
