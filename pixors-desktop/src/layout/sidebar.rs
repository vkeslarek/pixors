use iced::widget::container;
use iced::{Background, Border, Element, Length};
use crate::theme::BORDER_SUBTLE;

pub struct Sidebar<'a, Message> {
    content: Element<'a, Message>,
    width: f32,
    background: iced::Color,
}

pub fn sidebar<'a, Message>(content: impl Into<Element<'a, Message>>) -> Sidebar<'a, Message> {
    Sidebar {
        content: content.into(),
        width: 60.0,
        background: crate::theme::BG_BASE,
    }
}

impl<'a, Message> Sidebar<'a, Message> {
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn background(mut self, color: iced::Color) -> Self {
        self.background = color;
        self
    }
}

impl<'a, Message: 'a> From<Sidebar<'a, Message>> for Element<'a, Message> {
    fn from(s: Sidebar<'a, Message>) -> Self {
        container(s.content)
            .width(Length::Fixed(s.width))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(s.background)),
                border: Border {
                    width: 0.0,
                    color: BORDER_SUBTLE,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
