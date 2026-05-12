use iced::widget::{Column, slider, toggler};
use iced::{Element, Length};

use super::components::section_label;
use super::ExportDialog;
use crate::theme::TEXT_SECONDARY;

pub fn webp_options(dialog: &ExportDialog) -> Element<'static, super::Msg> {
    let lossless_toggle = toggler(dialog.webp.lossless)
        .label("Lossless")
        .on_toggle(super::Msg::WebPLossless);

    let quality_slider = if !dialog.webp.lossless {
        Some(
            slider(0.0..=100.0, dialog.webp.quality, |v| super::Msg::WebPQuality(v))
                .width(Length::Fill)
                .step(1.0),
        )
    } else {
        None
    };

    let mut col = Column::new()
        .spacing(12)
        .push(section_label("WEBP"))
        .push(lossless_toggle);

    if let Some(sl) = quality_slider {
        col = col.push(section_label("QUALITY")).push(sl);
    }

    col.into()
}
