use iced::widget::{Column, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::DANGER;

use super::components::*;
use super::{ExportDialog, ExportFormat, Msg};

pub fn view(dialog: &ExportDialog) -> Element<'_, Msg> {
    let content = card_view(dialog);

    crate::modal::modal("Export Image", content)
        .width(520.0)
        .height(800.0)
        .on_close(Msg::Cancel)
        .into()
}

fn card_view(dialog: &ExportDialog) -> Element<'_, Msg> {
    // Format segmented toggle
    let fmt_section = Column::new()
        .spacing(12)
        .push(section_label("FORMAT"))
        .push(format_toggle(&dialog.format));

    // Per-format options
    let options = match dialog.format {
        ExportFormat::Png => super::png::png_options(dialog),
        ExportFormat::Tiff => super::tiff::tiff_options(dialog),
        ExportFormat::Jpeg => super::jpeg::jpeg_options(dialog),
        ExportFormat::WebP => super::webp::webp_options(dialog),
    };

    // Error line
    let mut body = Column::new()
        .spacing(20)
        .padding(Padding::new(24.0))
        .push(fmt_section)
        .push(options);

    if let Some(ref err) = dialog.error {
        body = body.push(text(err.as_str()).size(13).color(DANGER));
    }

    // Action buttons
    let cancel_btn = crate::components::button("Cancel")
        .variant(crate::components::ButtonVariant::Secondary)
        .on_press(Msg::Cancel);

    let mut export_btn =
        crate::components::button("Export").variant(crate::components::ButtonVariant::Primary);

    if dialog.error.is_none() {
        export_btn = export_btn.on_press(Msg::Export);
    }

    let btns = row![
        iced::widget::Space::new().width(Length::Fill),
        Element::from(cancel_btn),
        Element::from(export_btn),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    body = body.push(btns);

    scrollable(body).into()
}
