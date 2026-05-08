use iced::widget::{button, checkbox, pick_list, row, slider, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

use super::presets::TiffLayoutKind;
use super::{ExportFormat, Msg};
use crate::theme::{
    ACCENT, BG_HOVER, BG_SURFACE, BORDER, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

pub fn section_label(label: &'static str) -> Element<'static, Msg> {
    text(label).size(12).color(TEXT_MUTED).into()
}

pub fn format_toggle(current: &ExportFormat) -> Element<'_, Msg> {
    let png_selected = matches!(current, ExportFormat::Png);
    let tiff_selected = matches!(current, ExportFormat::Tiff);

    let btn = |label: &'static str, selected: bool, msg: Msg, is_left: bool| {
        let (bg, fg, border_col) = if selected {
            (ACCENT, Color::WHITE, ACCENT)
        } else {
            (BG_SURFACE, TEXT_SECONDARY, BORDER)
        };

        button(text(label).size(13).color(fg))
            .padding([6, 24])
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
            "PNG",
            png_selected,
            Msg::FormatChanged(ExportFormat::Png),
            true
        ),
        btn(
            "TIFF",
            tiff_selected,
            Msg::FormatChanged(ExportFormat::Tiff),
            false
        )
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
    let pl = pick_list(options, Some(selected), msg)
        .style(|_, status| pick_list::Style {
            text_color: TEXT_PRIMARY,
            placeholder_color: TEXT_MUTED,
            handle_color: TEXT_SECONDARY,
            background: Background::Color(
                if matches!(
                    status,
                    pick_list::Status::Hovered | pick_list::Status::Opened { .. }
                ) {
                    BG_HOVER
                } else {
                    BG_SURFACE
                },
            ),
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 6.0.into(),
            },
        })
        .padding([6, 10])
        .text_size(13);

    row![text(label).size(13).color(TEXT_SECONDARY).width(140), pl]
        .align_y(Alignment::Center)
        .into()
}

pub fn labeled_checkbox<M: Clone + 'static>(
    label: &'static str,
    checked: bool,
    msg: impl Fn(bool) -> M + 'static,
) -> Element<'static, M> {
    let cb = checkbox(checked).on_toggle(msg).style(move |_, status| {
        let is_checked = checked;
        let hovered = matches!(status, iced::widget::checkbox::Status::Hovered { .. });
        iced::widget::checkbox::Style {
            background: Background::Color(if is_checked {
                if hovered {
                    Color::from_rgb(0.45, 0.60, 0.95)
                } else {
                    ACCENT
                }
            } else {
                if hovered { BG_HOVER } else { BG_SURFACE }
            }),
            icon_color: Color::WHITE,
            border: Border {
                color: if is_checked { ACCENT } else { BORDER },
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: None,
        }
    });

    row![text(label).size(13).color(TEXT_SECONDARY).width(140), cb]
        .align_y(Alignment::Center)
        .into()
}

pub fn labeled_slider<'a>(
    label: &'static str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    msg: impl Fn(f32) -> Msg + 'a,
) -> Element<'a, Msg> {
    let val_label = text(format!("{}", value as u8))
        .size(13)
        .color(TEXT_PRIMARY)
        .width(28);

    let sl = slider(range, value, msg)
        .step(1.0)
        .width(Length::Fill)
        .style(|_, status| iced::widget::slider::Style {
            rail: iced::widget::slider::Rail {
                backgrounds: (Background::Color(ACCENT), Background::Color(BG_HOVER)),
                width: 4.0,
                border: Border::default(),
            },
            handle: iced::widget::slider::Handle {
                shape: iced::widget::slider::HandleShape::Circle { radius: 7.0 },
                background: Background::Color(
                    if matches!(
                        status,
                        iced::widget::slider::Status::Hovered
                            | iced::widget::slider::Status::Dragged
                    ) {
                        Color::from_rgb(0.45, 0.60, 0.95)
                    } else {
                        ACCENT
                    },
                ),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        });

    row![
        text(label).size(13).color(TEXT_SECONDARY).width(140),
        sl,
        val_label,
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

pub fn labeled_text_input<'a>(
    label: &'static str,
    value: &'a str,
    msg: impl Fn(String) -> Msg + 'a,
) -> Element<'a, Msg> {
    let ti = text_input("", value)
        .on_input(msg)
        .padding([6, 10])
        .size(13)
        .style(|_, status| iced::widget::text_input::Style {
            background: Background::Color(BG_SURFACE),
            border: Border {
                color: if matches!(status, iced::widget::text_input::Status::Focused { .. }) {
                    ACCENT
                } else {
                    BORDER
                },
                width: 1.0,
                radius: 6.0.into(),
            },
            icon: TEXT_MUTED,
            placeholder: TEXT_MUTED,
            value: TEXT_PRIMARY,
            selection: Color::from_rgba(0.388, 0.533, 0.949, 0.3),
        });

    row![text(label).size(13).color(TEXT_SECONDARY).width(140), ti]
        .align_y(Alignment::Center)
        .into()
}
