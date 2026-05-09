use iced::widget::text_input;
use iced::{Background, Border, Color, Element};
use crate::theme::{BG_BASE, BG_ELEVATED, BORDER, BORDER_SUBTLE, TEXT_PRIMARY, TEXT_MUTED};

pub fn custom_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
    on_change: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    iced::widget::text_input(placeholder, value)
        .on_input(on_change)
        .size(13)
        .padding([6, 8])
        .style(move |_theme, status| {
            let bg = match status {
                text_input::Status::Focused { .. } => BG_ELEVATED,
                _ => BG_BASE,
            };

            let border_color = match status {
                text_input::Status::Focused { .. } => BORDER,
                _ => BORDER_SUBTLE,
            };

            text_input::Style {
                background: Background::Color(bg),
                border: Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: border_color,
                },
                icon: TEXT_MUTED,
                placeholder: TEXT_MUTED,
                value: TEXT_PRIMARY,
                selection: Color::from_rgba(1.0, 1.0, 1.0, 0.2),
            }
        })
        .into()
}
