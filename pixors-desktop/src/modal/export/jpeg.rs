use iced::Element;
use iced::widget::Column;

use super::ExportDialog;

pub fn jpeg_options(dialog: &ExportDialog) -> Element<'static, super::Msg> {
    Column::new()
        .spacing(12)
        .push(
            crate::components::slider("Quality", dialog.jpeg.quality as f32, 5.0..=100.0, |v| {
                super::Msg::JpegQuality(v as u8)
            })
            .step(5.0),
        )
        .into()
}
