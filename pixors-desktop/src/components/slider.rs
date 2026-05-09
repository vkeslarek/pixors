use crate::theme::{TEXT_MUTED, TEXT_PRIMARY};
use iced::widget::{column, row, slider as iced_slider, text};
use iced::{Alignment, Element, Length};
use std::ops::RangeInclusive;

pub struct Slider<'a, Message> {
    label: String,
    value: f32,
    range: RangeInclusive<f32>,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    value_format: Box<dyn Fn(f32) -> String + 'a>,
}

pub fn slider<'a, Message>(
    label: impl Into<String>,
    value: f32,
    range: RangeInclusive<f32>,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Slider<'a, Message> {
    Slider {
        label: label.into(),
        value,
        range,
        on_change: Box::new(on_change),
        value_format: Box::new(|v| format!("{:.2}", v)),
    }
}

impl<'a, Message> Slider<'a, Message> {
    pub fn value_format(mut self, format_fn: impl Fn(f32) -> String + 'a) -> Self {
        self.value_format = Box::new(format_fn);
        self
    }
}

impl<'a, Message: Clone + 'a> From<Slider<'a, Message>> for Element<'a, Message> {
    fn from(s: Slider<'a, Message>) -> Self {
        column![
            row![
                text(s.label).size(12).color(TEXT_PRIMARY),
                iced::widget::Space::new().width(Length::Fill),
                text((s.value_format)(s.value))
                    .size(12)
                    .color(TEXT_MUTED)
                    .font(iced::Font {
                        family: iced::font::Family::Monospace,
                        ..Default::default()
                    }),
            ]
            .align_y(Alignment::Center),
            iced_slider(s.range, s.value, s.on_change).step(1.0_f32),
        ]
        .spacing(8)
        .into()
    }
}
