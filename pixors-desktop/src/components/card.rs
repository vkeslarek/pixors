use crate::theme::{BG_SURFACE, BORDER_SUBTLE, TEXT_PRIMARY};
use iced::widget::{column, container, text};
use iced::{Background, Border, Element, Length, Padding};

pub struct Card<'a, Message> {
    title: Option<String>,
    content: Element<'a, Message>,
}

pub fn card<'a, Message>(content: impl Into<Element<'a, Message>>) -> Card<'a, Message> {
    Card {
        title: None,
        content: content.into(),
    }
}

impl<'a, Message> Card<'a, Message> {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl<'a, Message: 'a> From<Card<'a, Message>> for Element<'a, Message> {
    fn from(c: Card<'a, Message>) -> Self {
        let mut col = column![];

        if let Some(title_str) = c.title {
            col = col.push(
                container(text(title_str).size(14).color(TEXT_PRIMARY))
                    .padding(Padding::from([12, 16]))
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        border: Border {
                            color: BORDER_SUBTLE,
                            width: 1.0,
                            radius: Default::default(),
                        },
                        ..Default::default()
                    }),
            );
        }

        col = col.push(container(c.content).padding(16).width(Length::Fill));

        container(col)
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
