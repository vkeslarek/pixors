use iced::widget::{container, text};
use iced::{Background, Border, Color, Element, Padding};

#[derive(Default)]
pub enum BadgeVariant {
    Info,
    Success,
    Warning,
    Danger,
    #[default]
    Neutral,
}


pub struct Badge<'a, Message> {
    label: String,
    variant: BadgeVariant,
    _phantom: std::marker::PhantomData<&'a Message>,
}

pub fn badge<'a, Message>(label: impl Into<String>) -> Badge<'a, Message> {
    Badge {
        label: label.into(),
        variant: BadgeVariant::Neutral,
        _phantom: std::marker::PhantomData,
    }
}

impl<'a, Message> Badge<'a, Message> {
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl<'a, Message: 'a> From<Badge<'a, Message>> for Element<'a, Message> {
    fn from(b: Badge<'a, Message>) -> Self {
        let (bg, text_color, border_color) = match b.variant {
            BadgeVariant::Info => (
                Color::from_rgba(0.2, 0.4, 0.8, 0.2),
                Color::from_rgb(0.5, 0.7, 1.0),
                Color::from_rgba(0.2, 0.4, 0.8, 0.5),
            ),
            BadgeVariant::Success => (
                Color::from_rgba(0.2, 0.8, 0.2, 0.2),
                Color::from_rgb(0.4, 0.9, 0.4),
                Color::from_rgba(0.2, 0.8, 0.2, 0.5),
            ),
            BadgeVariant::Warning => (
                Color::from_rgba(0.8, 0.6, 0.1, 0.2),
                Color::from_rgb(1.0, 0.8, 0.3),
                Color::from_rgba(0.8, 0.6, 0.1, 0.5),
            ),
            BadgeVariant::Danger => (
                Color::from_rgba(0.9, 0.1, 0.1, 0.2),
                Color::from_rgb(1.0, 0.4, 0.4),
                Color::from_rgba(0.9, 0.1, 0.1, 0.5),
            ),
            BadgeVariant::Neutral => (
                crate::theme::BG_ELEVATED,
                crate::theme::TEXT_MUTED,
                crate::theme::BORDER_SUBTLE,
            ),
        };

        container(text(b.label).size(10).color(text_color))
            .padding(Padding::from([2, 6]))
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
