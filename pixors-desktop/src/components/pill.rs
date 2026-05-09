use crate::theme::{BG_ELEVATED, BORDER, OK_GREEN, TEXT_SECONDARY};
use iced::border::Radius;
use iced::widget::{Space, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

pub struct Pill<'a, Message> {
    label: String,
    dot_color: Option<Color>,
    _phantom: std::marker::PhantomData<&'a Message>,
}

pub fn pill<'a, Message>(label: impl Into<String>) -> Pill<'a, Message> {
    Pill {
        label: label.into(),
        dot_color: Some(OK_GREEN),
        _phantom: std::marker::PhantomData,
    }
}

impl<'a, Message> Pill<'a, Message> {
    pub fn dot_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.dot_color = color.into();
        self
    }
}

impl<'a, Message: 'a> From<Pill<'a, Message>> for Element<'a, Message> {
    fn from(p: Pill<'a, Message>) -> Self {
        let mut r = row![].spacing(6).align_y(Alignment::Center);

        if let Some(color) = p.dot_color {
            let dot = container(
                Space::new()
                    .width(Length::Fixed(8.0))
                    .height(Length::Fixed(8.0)),
            )
            .style(move |_| container::Style {
                background: Some(Background::Color(color)),
                border: Border::default().rounded(999),
                ..Default::default()
            });
            r = r.push(dot);
        }

        r = r.push(text(p.label).size(10).color(TEXT_SECONDARY));

        container(r)
            .padding(Padding::from([4, 10]))
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
}
