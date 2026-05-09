use crate::icons::LUCIDE;
use crate::theme::{ACCENT, ACCENT_DIM, ACCENT_GLOW, BG_HOVER, TEXT_MUTED, TEXT_PRIMARY};
use iced::widget::{button as iced_button, container, text};
use iced::{Background, Border, Color, Element, Length};

pub struct IconButton<'a, Message> {
    icon: String,
    size: u16,
    width: Length,
    height: Length,
    is_active: bool,
    on_press: Option<Message>,
    _phantom: std::marker::PhantomData<&'a ()>,
}

pub fn icon_button<'a, Message>(icon: impl Into<String>) -> IconButton<'a, Message> {
    IconButton {
        icon: icon.into(),
        size: 16,
        width: Length::Shrink,
        height: Length::Shrink,
        is_active: false,
        on_press: None,
        _phantom: std::marker::PhantomData,
    }
}

impl<'a, Message> IconButton<'a, Message> {
    pub fn size(mut self, size: u16) -> Self {
        self.size = size;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }

    pub fn on_press(mut self, msg: Message) -> Self {
        self.on_press = Some(msg);
        self
    }
}

impl<'a, Message: Clone + 'a> From<IconButton<'a, Message>> for Element<'a, Message> {
    fn from(b: IconButton<'a, Message>) -> Self {
        let icon_color = if b.is_active { ACCENT } else { TEXT_MUTED };
        let inner = container(
            text(b.icon)
                .font(LUCIDE)
                .size(b.size as f32)
                .color(icon_color)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

        let mut iced_btn = iced_button(inner)
            .padding(6)
            .width(b.width)
            .height(b.height)
            .style(move |_, status| {
                let hovered = matches!(status, iced::widget::button::Status::Hovered);

                let bg = if b.is_active {
                    Some(ACCENT_DIM)
                } else if hovered {
                    Some(BG_HOVER)
                } else {
                    None
                };

                let text_color = if b.is_active {
                    ACCENT
                } else if hovered {
                    TEXT_PRIMARY
                } else {
                    TEXT_MUTED
                };

                let border_color = if b.is_active {
                    ACCENT_GLOW
                } else {
                    Color::TRANSPARENT
                };

                iced::widget::button::Style {
                    background: bg.map(Background::Color),
                    text_color,
                    border: Border {
                        radius: 6.0.into(),
                        width: if b.is_active { 1.0 } else { 0.0 },
                        color: border_color,
                    },
                    ..Default::default()
                }
            });

        if let Some(msg) = b.on_press {
            iced_btn = iced_btn.on_press(msg);
        }

        iced_btn.into()
    }
}
