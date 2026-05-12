use iced::widget::{Column, slider};
use iced::{Element, Length};

use super::components::section_label;
use super::ExportDialog;

pub fn jpeg_options(dialog: &ExportDialog) -> Element<'static, super::Msg> {
    Column::new()
        .spacing(12)
        .push(section_label("QUALITY"))
        .push(
            slider(1.0..=100.0, dialog.jpeg.quality as f32, |v| {
                super::Msg::JpegQuality(v as u8)
            })
            .width(Length::Fill)
            .step(1.0),
        )
        .into()
}
