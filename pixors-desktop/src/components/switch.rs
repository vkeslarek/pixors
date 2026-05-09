use crate::theme::TEXT_MUTED;
use iced::widget::{column, container, text, toggler as iced_toggler};
use iced::{Element, Padding};

pub struct Switch<'a, Message> {
    label: String,
    is_toggled: bool,
    on_toggle: Box<dyn Fn(bool) -> Message + 'a>,
    description: Option<String>,
}

pub fn switch<'a, Message>(
    label: impl Into<String>,
    is_toggled: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Switch<'a, Message> {
    Switch {
        label: label.into(),
        is_toggled,
        on_toggle: Box::new(on_toggle),
        description: None,
    }
}

impl<'a, Message> Switch<'a, Message> {
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

impl<'a, Message: Clone + 'a> From<Switch<'a, Message>> for Element<'a, Message> {
    fn from(s: Switch<'a, Message>) -> Self {
        let t = iced_toggler(s.is_toggled)
            .label(s.label)
            .on_toggle(s.on_toggle)
            .size(20.0)
            .text_size(13)
            .text_alignment(iced::alignment::Horizontal::Left)
            .spacing(12);

        if let Some(desc) = s.description {
            column![
                t,
                container(text(desc).size(11).color(TEXT_MUTED)).padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 42.0
                }) // Align with label text
            ]
            .spacing(4)
            .into()
        } else {
            t.into()
        }
    }
}
