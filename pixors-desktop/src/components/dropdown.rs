use iced::widget::pick_list;
use iced::{Background, Border, Color, Element};
use crate::theme::{BG_BASE, BG_ELEVATED, BG_HOVER, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY};

pub fn dropdown<'a, T, Message, L, V>(
    options: L,
    selected: Option<V>,
    on_selected: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    V: std::borrow::Borrow<T> + 'a,
    Message: Clone + 'a,
{
    pick_list(options, selected, on_selected)
        .text_size(13)
        .padding([6, 12])
        .style(move |_theme, status| {
            let bg = match status {
                pick_list::Status::Hovered => BG_HOVER,
                pick_list::Status::Opened { .. } => BG_ELEVATED,
                _ => BG_BASE,
            };

            pick_list::Style {
                text_color: TEXT_PRIMARY,
                placeholder_color: TEXT_MUTED,
                handle_color: TEXT_MUTED,
                background: Background::Color(bg),
                border: Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: BORDER_SUBTLE,
                },
            }
        })
        .into()
}
