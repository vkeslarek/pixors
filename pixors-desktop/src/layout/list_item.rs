use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Padding};
use crate::theme::{BG_HOVER, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY};

pub struct ListItem<'a, Message> {
    title: String,
    subtitle: Option<String>,
    leading: Option<Element<'a, Message>>,
    trailing: Option<Element<'a, Message>>,
    on_press: Option<Message>,
    padding: Padding,
}

pub fn list_item<'a, Message>(title: impl Into<String>) -> ListItem<'a, Message> {
    ListItem {
        title: title.into(),
        subtitle: None,
        leading: None,
        trailing: None,
        on_press: None,
        padding: Padding::from([12, 16]),
    }
}

impl<'a, Message> ListItem<'a, Message> {
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn leading(mut self, leading: impl Into<Element<'a, Message>>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<Element<'a, Message>>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    pub fn on_press(mut self, msg: Message) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }
}

impl<'a, Message: Clone + 'a> From<ListItem<'a, Message>> for Element<'a, Message> {
    fn from(item: ListItem<'a, Message>) -> Self {
        let mut r = row![].align_y(Alignment::Center).spacing(12);

        if let Some(leading) = item.leading {
            r = r.push(leading);
        }

        let mut text_col = column![text(item.title).size(13).color(TEXT_PRIMARY)].spacing(2);

        if let Some(subtitle) = item.subtitle {
            text_col = text_col.push(text(subtitle).size(11).color(TEXT_MUTED));
        }

        r = r.push(
            container(text_col)
                .width(Length::Fill)
                .align_y(iced::alignment::Vertical::Center),
        );

        if let Some(trailing) = item.trailing {
            r = r.push(trailing);
        }

        let mut btn = iced::widget::button(r)
            .padding(item.padding)
            .width(Length::Fill)
            .style(|_theme, status| {
                let bg = match status {
                    iced::widget::button::Status::Hovered => Some(Background::Color(BG_HOVER)),
                    _ => Some(Background::Color(BG_SURFACE)),
                };
                iced::widget::button::Style {
                    background: bg,
                    border: Border {
                        width: 1.0,
                        color: BORDER_SUBTLE,
                        radius: 0.0.into(),
                    },
                    text_color: TEXT_PRIMARY,
                    ..Default::default()
                }
            });

        if let Some(msg) = item.on_press {
            btn = btn.on_press(msg);
        }

        btn.into()
    }
}
