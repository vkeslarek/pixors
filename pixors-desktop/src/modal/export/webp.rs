use iced::Element;
use iced::widget::Column;

use super::ExportDialog;
use super::components::{labeled_checkbox, labeled_slider, section_label};

pub fn webp_options(dialog: &ExportDialog) -> Element<'static, super::Msg> {
    let mut col = Column::new()
        .spacing(12)
        .push(section_label("COMPRESSION"))
        .push(labeled_checkbox(
            "Lossless",
            dialog.webp.lossless,
            super::Msg::WebPLossless,
        ));

    if !dialog.webp.lossless {
        col = col.push(labeled_slider(
            "Quality",
            5.0..=100.0,
            5.0,
            dialog.webp.quality,
            super::Msg::WebPQuality,
        ));
    }

    col.into()
}
