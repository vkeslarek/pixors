use crate::theme::{
    ACCENT, ACCENT_GLOW, BG_BASE, BG_ELEVATED, BG_HOVER, BG_SURFACE, BORDER, BORDER_SUBTLE, DANGER,
    TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};
use iced::widget::{button as iced_button, text};
use iced::{Background, Border, Color, Element, Length, Padding, alignment::Horizontal};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
}

pub struct Button<'a, Message> {
    label: String,
    variant: ButtonVariant,
    size: ButtonSize,
    width: Length,
    on_press: Option<Message>,
    _phantom: std::marker::PhantomData<&'a ()>,
}

pub fn button<'a, Message>(label: impl Into<String>) -> Button<'a, Message> {
    Button {
        label: label.into(),
        variant: ButtonVariant::Primary,
        size: ButtonSize::Md,
        width: Length::Shrink,
        on_press: None,
        _phantom: std::marker::PhantomData,
    }
}

impl<'a, Message> Button<'a, Message> {
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn on_press(mut self, msg: Message) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

impl<'a, Message: Clone + 'a> From<Button<'a, Message>> for Element<'a, Message> {
    fn from(b: Button<'a, Message>) -> Self {
        let padding = match b.size {
            ButtonSize::Sm => Padding::from([4, 8]),
            ButtonSize::Md => Padding::from([6, 12]),
            ButtonSize::Lg => Padding::from([8, 16]),
        };

        let text_size = match b.size {
            ButtonSize::Sm => 12,
            ButtonSize::Md => 13,
            ButtonSize::Lg => 14,
        };

        let variant = b.variant;
        let style = move |_theme: &iced::Theme,
                          state: iced::widget::button::Status|
              -> iced::widget::button::Style {
            let (bg, text_color, border_color) = match variant {
                ButtonVariant::Primary => match state {
                    iced::widget::button::Status::Hovered => {
                        (Some(ACCENT_GLOW), TEXT_PRIMARY, ACCENT)
                    }
                    iced::widget::button::Status::Disabled => {
                        (Some(BG_ELEVATED), TEXT_MUTED, BORDER_SUBTLE)
                    }
                    _ => (Some(ACCENT), TEXT_PRIMARY, ACCENT),
                },
                ButtonVariant::Secondary => match state {
                    iced::widget::button::Status::Hovered => (Some(BG_HOVER), TEXT_PRIMARY, BORDER),
                    iced::widget::button::Status::Disabled => {
                        (Some(BG_BASE), TEXT_MUTED, BORDER_SUBTLE)
                    }
                    _ => (Some(BG_SURFACE), TEXT_PRIMARY, BORDER),
                },
                ButtonVariant::Ghost => match state {
                    iced::widget::button::Status::Hovered => {
                        (Some(BG_HOVER), TEXT_PRIMARY, Color::TRANSPARENT)
                    }
                    iced::widget::button::Status::Disabled => {
                        (None, TEXT_MUTED, Color::TRANSPARENT)
                    }
                    _ => (None, TEXT_SECONDARY, Color::TRANSPARENT),
                },
                ButtonVariant::Danger => match state {
                    iced::widget::button::Status::Hovered => (
                        Some(Color::from_rgba(0.9, 0.1, 0.1, 0.8)),
                        TEXT_PRIMARY,
                        DANGER,
                    ),
                    iced::widget::button::Status::Disabled => {
                        (Some(BG_BASE), TEXT_MUTED, BORDER_SUBTLE)
                    }
                    _ => (Some(DANGER), TEXT_PRIMARY, DANGER),
                },
            };

            iced::widget::button::Style {
                background: bg.map(Background::Color),
                text_color,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        };

        let mut iced_btn = iced_button(text(b.label).size(text_size).align_x(Horizontal::Center))
            .style(style)
            .padding(padding)
            .width(b.width);

        if let Some(msg) = b.on_press {
            iced_btn = iced_btn.on_press(msg);
        }

        iced_btn.into()
    }
}
