use iced::widget::{button, row, text};
use iced::{Alignment, Background, Border, Color, Element};

use super::presets::TiffLayoutKind;
use super::{ExportFormat, Msg};
use crate::theme::{ACCENT, BG_HOVER, BG_SURFACE, BORDER, TEXT_MUTED, TEXT_SECONDARY};

pub fn section_label(label: &'static str) -> Element<'static, Msg> {
    text(label).size(12).color(TEXT_MUTED).into()
}

pub fn format_toggle(current: &ExportFormat) -> Element<'_, Msg> {
    let btn = |label: &'static str, format: ExportFormat, is_left: bool, is_right: bool| {
        let selected = *current == format;
        let (bg, fg, border_col) = if selected {
            (ACCENT, Color::WHITE, ACCENT)
        } else {
            (BG_SURFACE, TEXT_SECONDARY, BORDER)
        };

        let radius = if is_left {
            iced::border::Radius { top_left: 6.0, top_right: 0.0, bottom_right: 0.0, bottom_left: 6.0 }
        } else if is_right {
            iced::border::Radius { top_left: 0.0, top_right: 6.0, bottom_right: 6.0, bottom_left: 0.0 }
        } else {
            iced::border::Radius { top_left: 0.0, top_right: 0.0, bottom_right: 0.0, bottom_left: 0.0 }
        };

        button(text(label).size(13).color(fg))
            .padding([6, 18])
            .style(move |_, status| {
                let actual_bg = if !selected && matches!(status, button::Status::Hovered) { BG_HOVER } else { bg };
                button::Style {
                    background: Some(Background::Color(actual_bg)),
                    border: Border { color: border_col, width: 1.0, radius },
                    text_color: fg,
                    ..Default::default()
                }
            })
            .on_press(Msg::FormatChanged(format))
    };

    row![
        btn("PNG", ExportFormat::Png, true, false),
        btn("TIFF", ExportFormat::Tiff, false, false),
        btn("JPEG", ExportFormat::Jpeg, false, false),
        btn("WebP", ExportFormat::WebP, false, true),
    ]
    .into()
}

pub fn layout_toggle(current: TiffLayoutKind) -> Element<'static, Msg> {
    let strip_selected = matches!(current, TiffLayoutKind::Strip);
    let tile_selected = matches!(current, TiffLayoutKind::Tile);

    let btn = |label: &'static str, selected: bool, msg: Msg, is_left: bool| {
        let (bg, fg, border_col) = if selected {
            (ACCENT, Color::WHITE, ACCENT)
        } else {
            (BG_SURFACE, TEXT_SECONDARY, BORDER)
        };

        button(text(label).size(13).color(fg))
            .padding([6, 16])
            .style(move |_, status| {
                let actual_bg = if !selected && matches!(status, button::Status::Hovered) {
                    BG_HOVER
                } else {
                    bg
                };

                let radius = if is_left {
                    iced::border::Radius {
                        top_left: 6.0,
                        top_right: 0.0,
                        bottom_right: 0.0,
                        bottom_left: 6.0,
                    }
                } else {
                    iced::border::Radius {
                        top_left: 0.0,
                        top_right: 6.0,
                        bottom_right: 6.0,
                        bottom_left: 0.0,
                    }
                };

                button::Style {
                    background: Some(Background::Color(actual_bg)),
                    border: Border {
                        color: border_col,
                        width: 1.0,
                        radius,
                    },
                    text_color: fg,
                    ..Default::default()
                }
            })
            .on_press(msg)
    };

    row![
        btn(
            "Strip",
            strip_selected,
            Msg::TiffLayoutKind(TiffLayoutKind::Strip),
            true
        ),
        btn(
            "Tile",
            tile_selected,
            Msg::TiffLayoutKind(TiffLayoutKind::Tile),
            false
        )
    ]
    .into()
}

pub fn labeled_pick<'a, T>(
    label: &'static str,
    options: &'static [T],
    selected: T,
    msg: impl Fn(T) -> Msg + 'static,
) -> Element<'a, Msg>
where
    T: std::fmt::Display + PartialEq + Clone + 'static,
{
    row![
        text(label).size(13).color(TEXT_SECONDARY).width(140),
        crate::components::dropdown(options, Some(selected), msg)
    ]
    .align_y(Alignment::Center)
    .into()
}

pub fn labeled_checkbox<M: Clone + 'static>(
    label: &'static str,
    checked: bool,
    msg: impl Fn(bool) -> M + 'static,
) -> Element<'static, M> {
    row![
        text(label).size(13).color(TEXT_SECONDARY).width(140),
        crate::components::switch("", checked, msg)
    ]
    .align_y(Alignment::Center)
    .into()
}

pub fn labeled_slider<'a>(
    label: &'static str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    msg: impl Fn(f32) -> Msg + 'a,
) -> Element<'a, Msg> {
    row![crate::components::slider(label, value, range, msg).value_format(|v| format!("{:.0}", v))]
        .align_y(Alignment::Center)
        .into()
}

pub fn labeled_text_input<'a>(
    label: &'static str,
    value: &'a str,
    msg: impl Fn(String) -> Msg + 'a,
) -> Element<'a, Msg> {
    row![
        text(label).size(13).color(TEXT_SECONDARY).width(140),
        crate::components::input("", value, msg)
    ]
    .align_y(Alignment::Center)
    .into()
}
