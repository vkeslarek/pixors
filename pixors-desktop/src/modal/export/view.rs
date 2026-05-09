use iced::widget::{Column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::theme::{
    BG_ELEVATED, BORDER, BORDER_SUBTLE, DANGER, TEXT_PRIMARY,
};

use super::components::*;
use super::{ExportDialog, ExportFormat, Msg};

pub fn view(dialog: &ExportDialog) -> Element<'_, Msg> {
    let card = card_view(dialog);

    container(scrollable(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding::new(40.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.65))),
            ..Default::default()
        })
        .into()
}

fn card_view(dialog: &ExportDialog) -> Element<'_, Msg> {
    // Header row
    let header = row![
        text("Export Image").size(16).color(TEXT_PRIMARY),
        iced::widget::Space::new().width(Length::Fill),
        Element::from(
            crate::components::icon_button(crate::icons::X)
                .size(14)
                .on_press(Msg::Cancel)
        ),
    ]
    .align_y(Alignment::Center);

    let divider = container(text(""))
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(Background::Color(BORDER_SUBTLE)),
            ..Default::default()
        });

    // Format segmented toggle
    let fmt_section = Column::new()
        .spacing(12)
        .push(section_label("FORMAT"))
        .push(format_toggle(&dialog.format));

    // Per-format options
    let options = match dialog.format {
        ExportFormat::Png => super::png::png_options(dialog),
        ExportFormat::Tiff => super::tiff::tiff_options(dialog),
    };

    // Error line
    let mut body = Column::new()
        .spacing(20)
        .padding(Padding::new(24.0))
        .push(header)
        .push(divider)
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

    container(body)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_ELEVATED)),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .width(520)
        .height(Length::Shrink)
        .into()
}
