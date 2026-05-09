use iced::widget::{button, column, container, row, scrollable, slider, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::panel::filter::Msg;
use crate::icons::{
    CIRCLE_SLASH, EYE, EYE_OFF, GRIP_VERTICAL, INFO, LUCIDE, PLUS, SUN, TRASH, UNDO,
};
use crate::theme::{
    ACCENT, BG_BASE, BG_ELEVATED, BG_HOVER, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};

pub fn view<'a>() -> Element<'a, Msg> {
    let toolbar = row![
        button(
            row![
                text(PLUS).font(LUCIDE).size(14).color(TEXT_SECONDARY),
                text("Add filter").size(13).color(TEXT_SECONDARY),
                Space::new().width(Length::Fill),
                container(text("⌘F").size(9).color(TEXT_MUTED))
                    .padding([2, 4])
                    .style(|_| container::Style {
                        background: Some(Background::Color(BG_BASE)),
                        border: Border::default().rounded(4),
                        ..Default::default()
                    })
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        )
        .width(Length::Fill)
        .padding([8, 12])
        .style(btn_style),
        Space::new().width(Length::Fixed(8.0)),
        button(
            row![
                text(EYE).font(LUCIDE).size(14).color(TEXT_MUTED),
                text("Bypass").size(13).color(TEXT_MUTED)
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        )
        .width(Length::Fill)
        .padding([8, 12])
        .style(btn_style)
    ]
    .padding(iced::Padding { top: 12.0, right: 16.0, bottom: 12.0, left: 16.0 });

    let filter1 = build_collapsed_filter(
        "01",
        "Gaussian Blur",
        "radius 12px • ",
        "80%",
        Color::from_rgb(0.7, 0.5, 0.6),
    );

    let filter2 = build_expanded_filter();

    let filter3 = build_disabled_filter(
        "03",
        "Color Lookup",
        "Color • Cinematic Wa...",
        Color::from_rgb(0.3, 0.25, 0.3),
    );

    let content = column![toolbar, filter1, filter2, filter3].spacing(0);

    let footer = container(row![
        row![
            container(Space::new().width(Length::Fixed(6.0)).height(Length::Fixed(6.0))).style(
                |_| container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.2, 0.8, 0.2))),
                    border: Border::default().rounded(3),
                    ..Default::default()
                }
            ),
            Space::new().width(Length::Fixed(6.0)),
            text("3 active").size(11).color(TEXT_SECONDARY),
            text(" • 12ms").size(11).color(TEXT_MUTED),
        ]
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill),
        button(text("Save preset").size(12).color(TEXT_SECONDARY))
            .padding([6, 10])
            .style(|t, s| {
                let mut st = btn_style(t, s);
                st.background = Some(Background::Color(BG_BASE));
                st.border = Border {
                    color: BORDER_SUBTLE,
                    width: 1.0,
                    radius: 4.0.into(),
                };
                st
            }),
        Space::new().width(Length::Fixed(6.0)),
        button(text("Flatten").size(12).color(Color::WHITE))
            .padding([6, 12])
            .style(primary_btn_style),
    ]
    .align_y(Alignment::Center))
    .padding([12, 16])
    .style(|_| container::Style {
        border: Border {
            width: 1.0,
            color: BORDER_SUBTLE,
            ..Border::default()
        },
        background: Some(Background::Color(BG_SURFACE)),
        ..Default::default()
    });

    container(column![
        scrollable(content).height(Length::Fill).width(Length::Fill),
        container(footer).width(Length::Fill)
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(BG_SURFACE)),
        ..Default::default()
    })
    .into()
}

fn build_collapsed_filter<'a>(
    num: &'static str,
    title: &'static str,
    subtitle1: &'static str,
    subtitle2: &'static str,
    color: Color,
) -> Element<'a, Msg> {
    let icon_sq = container(Space::new().width(Length::Fixed(24.0)).height(Length::Fixed(24.0)))
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border::default().rounded(4),
            ..Default::default()
        });

    let info = column![
        text(title).size(13).color(TEXT_PRIMARY),
        row![
            text(subtitle1).size(11).color(TEXT_MUTED),
            text(subtitle2).size(11).color(ACCENT),
        ]
    ]
    .spacing(2);

    let actions = row![
        text(INFO).font(LUCIDE).size(14).color(TEXT_MUTED),
        Space::new().width(Length::Fixed(12.0)),
        text(EYE).font(LUCIDE).size(14).color(TEXT_MUTED),
        Space::new().width(Length::Fixed(12.0)),
        text(TRASH).font(LUCIDE).size(14).color(TEXT_MUTED),
    ]
    .align_y(Alignment::Center);

    container(
        row![
            Space::new().width(Length::Fixed(8.0)), // space for absent grip
            text(num).size(9).color(TEXT_MUTED).font(iced::Font {
                family: iced::font::Family::Monospace,
                ..Default::default()
            }),
            Space::new().width(Length::Fixed(12.0)),
            icon_sq,
            Space::new().width(Length::Fixed(12.0)),
            info,
            Space::new().width(Length::Fill),
            actions
        ]
        .align_y(Alignment::Center),
    )
    .padding([12, 16])
    .style(|_| container::Style {
        border: Border {
            width: 1.0,
            color: BORDER_SUBTLE,
            ..Border::default()
        },
        ..Default::default()
    })
    .into()
}

fn build_expanded_filter<'a>() -> Element<'a, Msg> {
    let icon_sq = container(Space::new().width(Length::Fixed(24.0)).height(Length::Fixed(24.0)))
        .style(move |_| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.8, 0.4, 0.6))),
            border: Border::default().rounded(4),
            ..Default::default()
        });

    let info = column![
        text("Smart Sharpen")
            .size(13)
            .color(TEXT_PRIMARY)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            }),
        row![text("amount 65 • 1.4px").size(11).color(TEXT_MUTED),]
    ]
    .spacing(2);

    let actions = row![
        text(INFO).font(LUCIDE).size(14).color(TEXT_MUTED),
        Space::new().width(Length::Fixed(12.0)),
        text(EYE).font(LUCIDE).size(14).color(TEXT_MUTED),
        Space::new().width(Length::Fixed(12.0)),
        text(TRASH).font(LUCIDE).size(14).color(TEXT_MUTED),
    ]
    .align_y(Alignment::Center);

    let header = row![
        text(GRIP_VERTICAL).font(LUCIDE).size(14).color(TEXT_MUTED),
        Space::new().width(Length::Fixed(2.0)),
        text("02").size(9).color(ACCENT).font(iced::Font {
            family: iced::font::Family::Monospace,
            ..Default::default()
        }),
        Space::new().width(Length::Fixed(12.0)),
        icon_sq,
        Space::new().width(Length::Fixed(12.0)),
        info,
        Space::new().width(Length::Fill),
        actions
    ]
    .align_y(Alignment::Center)
    .padding([12, 16]);

    let blend_dropdown = container(
        row![
            text("Normal").size(12).color(TEXT_PRIMARY),
            Space::new().width(Length::Fill),
            text(crate::icons::CHEVRON_DOWN)
                .font(LUCIDE)
                .size(14)
                .color(TEXT_MUTED)
        ]
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .width(Length::Fill)
    .style(|_| dropdown_style());

    let opacity_input = container(text("100%").size(12).color(TEXT_PRIMARY))
        .padding([8, 12])
        .width(Length::Fixed(60.0))
        .align_x(Alignment::End)
        .style(|_| dropdown_style());

    let controls = column![
        row![
            blend_dropdown,
            Space::new().width(Length::Fixed(8.0)),
            opacity_input,
        ],
        Space::new().height(Length::Fixed(16.0)),
        build_slider("Amount", "65%", 65.0, 0.0..=100.0),
        Space::new().height(Length::Fixed(12.0)),
        build_slider("Radius", "1.4 px", 1.4, 0.0..=5.0),
        Space::new().height(Length::Fixed(12.0)),
        build_slider_secondary("Threshold", "0", 0.0, 0.0..=255.0),
        Space::new().height(Length::Fixed(20.0)),
        row![
            button(
                container(
                    row![
                        text(CIRCLE_SLASH).font(LUCIDE).size(14).color(TEXT_MUTED),
                        text("Mask").size(11).color(TEXT_SECONDARY)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                )
                .width(Length::Fill)
                .center_x(Length::Fill)
            )
            .width(Length::Fill)
            .padding([6, 0])
            .style(ghost_btn_style),
            Space::new().width(Length::Fixed(6.0)),
            button(
                container(
                    row![
                        text(UNDO).font(LUCIDE).size(14).color(TEXT_MUTED),
                        text("Reset").size(11).color(TEXT_SECONDARY)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                )
                .width(Length::Fill)
                .center_x(Length::Fill)
            )
            .width(Length::Fill)
            .padding([6, 0])
            .style(ghost_btn_style),
            Space::new().width(Length::Fixed(6.0)),
            button(
                container(
                    row![
                        text(SUN).font(LUCIDE).size(14).color(TEXT_MUTED),
                        text("Presets").size(11).color(TEXT_SECONDARY)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                )
                .width(Length::Fill)
                .center_x(Length::Fill)
            )
            .width(Length::Fill)
            .padding([6, 0])
            .style(ghost_btn_style),
        ]
    ]
    .padding(iced::Padding { top: 0.0, right: 16.0, bottom: 16.0, left: 16.0 });

    let body = container(column![header, controls])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_BASE)),
            border: Border {
                width: 1.0,
                color: BORDER_SUBTLE,
                ..Border::default()
            },
            ..Default::default()
        });

    let active_border = container(Space::new())
        .width(Length::Fixed(2.0))
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(ACCENT)),
            ..Default::default()
        });

    container(row![active_border, body])
        .width(Length::Fill)
        .into()
}

fn build_disabled_filter<'a>(
    num: &'static str,
    title: &'static str,
    subtitle: &'static str,
    color: Color,
) -> Element<'a, Msg> {
    let icon_sq = container(Space::new().width(Length::Fixed(24.0)).height(Length::Fixed(24.0)))
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border::default().rounded(4),
            ..Default::default()
        });

    let info = column![
        text(title).size(13).color(TEXT_MUTED),
        text(subtitle).size(11).color(TEXT_MUTED),
    ]
    .spacing(2);

    let actions = row![
        text(INFO).font(LUCIDE).size(14).color(ACCENT),
        Space::new().width(Length::Fixed(12.0)),
        text(EYE_OFF).font(LUCIDE).size(14).color(ACCENT),
        Space::new().width(Length::Fixed(12.0)),
        text(TRASH).font(LUCIDE).size(14).color(TEXT_MUTED),
    ]
    .align_y(Alignment::Center);

    container(
        row![
            Space::new().width(Length::Fixed(8.0)),
            text(num).size(9).color(TEXT_MUTED).font(iced::Font {
                family: iced::font::Family::Monospace,
                ..Default::default()
            }),
            Space::new().width(Length::Fixed(12.0)),
            icon_sq,
            Space::new().width(Length::Fixed(12.0)),
            info,
            Space::new().width(Length::Fill),
            actions
        ]
        .align_y(Alignment::Center),
    )
    .padding([12, 16])
    .style(|_| container::Style {
        border: Border {
            width: 1.0,
            color: BORDER_SUBTLE,
            ..Border::default()
        },
        ..Default::default()
    })
    .into()
}

fn build_slider<'a>(
    label: &'static str,
    val_str: &'static str,
    val: f32,
    range: std::ops::RangeInclusive<f32>,
) -> Element<'a, Msg> {
    column![
        row![
            text(label).size(11).color(TEXT_SECONDARY),
            Space::new().width(Length::Fill),
            text(val_str).size(11).color(ACCENT),
        ],
        Space::new().height(Length::Fixed(6.0)),
        slider(range, val, |_| Msg::SetBlur(0.0))
    ]
    .into()
}

fn build_slider_secondary<'a>(
    label: &'static str,
    val_str: &'static str,
    val: f32,
    range: std::ops::RangeInclusive<f32>,
) -> Element<'a, Msg> {
    column![
        row![
            text(label).size(11).color(TEXT_SECONDARY),
            Space::new().width(Length::Fill),
            text(val_str).size(11).color(ACCENT),
        ],
        Space::new().height(Length::Fixed(6.0)),
        slider(range, val, |_| Msg::SetBlur(0.0))
    ]
    .into()
}

fn btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if hovered {
            BG_HOVER
        } else {
            BG_ELEVATED
        })),
        border: Border::default().rounded(6),
        text_color: TEXT_PRIMARY,
        ..Default::default()
    }
}

fn ghost_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if hovered {
            BG_HOVER
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.05)
        })),
        border: Border::default().rounded(6),
        text_color: TEXT_PRIMARY,
        ..Default::default()
    }
}

fn primary_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb(0.2, 0.6, 1.0)
        } else {
            ACCENT
        })),
        border: Border::default().rounded(6),
        text_color: Color::WHITE,
        ..Default::default()
    }
}

fn dropdown_style() -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_ELEVATED)),
        border: Border::default().rounded(6),
        ..Default::default()
    }
}
