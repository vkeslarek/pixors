use iced::widget::{Space, button, column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::icons::{
    EYE, EYE_OFF, GRIP_VERTICAL, INFO, LUCIDE, PLUS, TRASH,
};
use crate::theme::{
    ACCENT, BG_BASE, BG_ELEVATED, BG_HOVER, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};

#[derive(Debug, Clone)]
pub enum Msg {
    OpenFilterSearch,
    ToggleExpand(usize),
    DragStart(usize),
    DragHover(usize),
    DragDrop,
    SetBlur(f32),
    NoOp,
}

#[derive(Debug, Clone)]
pub struct FilterNode {
    pub title: String,
    pub subtitle1: String,
    pub subtitle2: String,
    pub color: Color,
    pub expanded: bool,
    pub disabled: bool,
}

#[derive(Debug, Clone)]
pub struct State {
    pub filters: Vec<FilterNode>,
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            filters: vec![
                FilterNode {
                    title: "Gaussian Blur".to_string(),
                    subtitle1: "radius 12px • ".to_string(),
                    subtitle2: "80%".to_string(),
                    color: Color::from_rgb(0.7, 0.5, 0.6),
                    expanded: false,
                    disabled: false,
                },
                FilterNode {
                    title: "Smart Sharpen".to_string(),
                    subtitle1: "amount 65 • 1.4px".to_string(),
                    subtitle2: "".to_string(),
                    color: Color::from_rgb(0.8, 0.4, 0.6),
                    expanded: true,
                    disabled: false,
                },
                FilterNode {
                    title: "Color Lookup".to_string(),
                    subtitle1: "Color • Cinematic Wa...".to_string(),
                    subtitle2: "".to_string(),
                    color: Color::from_rgb(0.3, 0.25, 0.3),
                    expanded: false,
                    disabled: true,
                },
            ],
            drag_from: None,
            drag_over: None,
        }
    }
}

impl State {
    pub fn update(&mut self, msg: Msg) -> Option<crate::panel::filter::Msg> {
        match msg {
            Msg::OpenFilterSearch => return Some(crate::panel::filter::Msg::OpenFilterSearch),
            Msg::ToggleExpand(idx) => {
                if let Some(f) = self.filters.get_mut(idx) {
                    f.expanded = !f.expanded;
                }
            }
            Msg::DragStart(idx) => {
                self.drag_from = Some(idx);
                self.drag_over = Some(idx);
            }
            Msg::DragHover(idx) => {
                if self.drag_from.is_some() {
                    self.drag_over = Some(idx);
                }
            }
            Msg::DragDrop => {
                if let (Some(from), Some(to)) = (self.drag_from, self.drag_over)
                    && from != to {
                        let f = self.filters.remove(from);
                        self.filters.insert(to, f);
                    }
                self.drag_from = None;
                self.drag_over = None;
            }
            Msg::SetBlur(_) | Msg::NoOp => {}
        }
        None
    }

    pub fn view<'a>(&'a self) -> Element<'a, Msg> {
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
            .style(|_theme, state| {
                let (bg, text_color, border_color) = match state {
                    iced::widget::button::Status::Hovered => (Some(BG_HOVER), TEXT_PRIMARY, crate::theme::BORDER),
                    _ => (Some(BG_SURFACE), TEXT_PRIMARY, crate::theme::BORDER),
                };
                iced::widget::button::Style {
                    background: bg.map(Background::Color),
                    text_color,
                    border: Border {
                        color: border_color,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(Msg::OpenFilterSearch)
        ]
        .padding(iced::Padding {
            top: 12.0,
            right: 16.0,
            bottom: 12.0,
            left: 16.0,
        });

        let mut filter_elements = Vec::new();
        for (i, f) in self.filters.iter().enumerate() {
            let num = format!("{:02}", i + 1);
            
            let is_dragged = self.drag_from == Some(i);
            let is_hover_target = self.drag_over == Some(i) && self.drag_from.is_some_and(|from| from != i);
            
            let el = if f.disabled {
                build_disabled_filter(i, &num, &f.title, &f.subtitle1, f.color)
            } else if f.expanded {
                build_expanded_filter(i, &num, &f.title, &f.subtitle1, f.color)
            } else {
                build_collapsed_filter(i, &num, &f.title, &f.subtitle1, &f.subtitle2, f.color)
            };
            
            let wrapper = container(el)
                .style(move |_| {
                    if is_hover_target {
                        container::Style {
                            background: Some(Background::Color(Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.30))),
                            ..Default::default()
                        }
                    } else if is_dragged {
                        container::Style {
                            background: Some(Background::Color(Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.10))),
                            ..Default::default()
                        }
                    } else {
                        container::Style::default()
                    }
                });
                
            let area = mouse_area(wrapper).on_enter(Msg::DragHover(i));
            filter_elements.push(area.into());
        }

        let filter_list = mouse_area(column(filter_elements).spacing(2).padding([4, 8]))
            .on_release(Msg::DragDrop);

        let content = column![toolbar, filter_list].spacing(0);

        let footer = container(
            column![
                row![
                    container(
                        Space::new()
                            .width(Length::Fixed(6.0))
                            .height(Length::Fixed(6.0))
                    )
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.2, 0.8, 0.2))),
                        border: Border::default().rounded(3),
                        ..Default::default()
                    }),
                    Space::new().width(Length::Fixed(6.0)),
                    text(format!("{} active", self.filters.iter().filter(|f| !f.disabled).count())).size(11).color(TEXT_SECONDARY),
                    text(" • 12ms").size(11).color(TEXT_MUTED),
                ]
                .align_y(Alignment::Center),
                Space::new().height(Length::Fixed(12.0)),
                crate::components::button::button("Flatten")
                    .variant(crate::components::button::ButtonVariant::Primary)
                    .width(Length::Fill)
                    .on_press(Msg::NoOp),
            ]
        )
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
}

fn build_collapsed_filter<'a>(
    idx: usize,
    num: &str,
    title: &str,
    subtitle1: &str,
    subtitle2: &str,
    color: Color,
) -> Element<'a, Msg> {
    let icon_sq = container(
        Space::new()
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(color)),
        border: Border::default().rounded(3),
        ..Default::default()
    });

    let info = column![
        text(title.to_string()).size(11).color(TEXT_SECONDARY),
        row![
            text(subtitle1.to_string()).size(9).color(TEXT_MUTED),
            text(subtitle2.to_string()).size(9).color(ACCENT),
        ]
    ]
    .spacing(2);

    let actions = row![
        crate::components::icon_button::icon_button(INFO).size(12).on_press(Msg::NoOp),
        crate::components::icon_button::icon_button(EYE).size(12).on_press(Msg::NoOp),
        crate::components::icon_button::icon_button(TRASH).size(12).on_press(Msg::NoOp),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let grip = mouse_area(
        container(text(GRIP_VERTICAL).font(LUCIDE).size(12).color(TEXT_MUTED))
            .padding([4, 4])
    )
    .on_press(Msg::DragStart(idx));

    let content_btn = button(
        row![
            Space::new().width(Length::Fixed(4.0)),
            text(num.to_string()).size(9).color(TEXT_MUTED).font(iced::Font {
                family: iced::font::Family::Monospace,
                ..Default::default()
            }),
            Space::new().width(Length::Fixed(8.0)),
            icon_sq,
            Space::new().width(Length::Fixed(8.0)),
            info,
        ]
        .align_y(Alignment::Center)
    )
    .width(Length::Fill)
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::ToggleExpand(idx));

    container(
        row![
            grip,
            content_btn,
            actions
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 8])
    .style(|_| container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border::default().rounded(4),
        ..Default::default()
    })
    .into()
}

fn build_expanded_filter<'a>(
    idx: usize,
    num: &str,
    title: &str,
    subtitle1: &str,
    color: Color,
) -> Element<'a, Msg> {
    let icon_sq = container(
        Space::new()
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: 3.0.into(),
            width: 2.0,
            color: ACCENT,
        },
        ..Default::default()
    });

    let info = column![
        text(title.to_string())
            .size(11)
            .color(TEXT_PRIMARY)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            }),
        row![text(subtitle1.to_string()).size(9).color(TEXT_MUTED),]
    ]
    .spacing(2);

    let actions = row![
        crate::components::icon_button::icon_button(INFO).size(12).on_press(Msg::NoOp),
        crate::components::icon_button::icon_button(EYE).size(12).on_press(Msg::NoOp),
        crate::components::icon_button::icon_button(TRASH).size(12).on_press(Msg::NoOp),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let grip = mouse_area(
        container(text(GRIP_VERTICAL).font(LUCIDE).size(12).color(TEXT_MUTED))
            .padding([4, 4])
    )
    .on_press(Msg::DragStart(idx));

    let content_btn = button(
        row![
            Space::new().width(Length::Fixed(4.0)),
            text(num.to_string()).size(9).color(ACCENT).font(iced::Font {
                family: iced::font::Family::Monospace,
                ..Default::default()
            }),
            Space::new().width(Length::Fixed(8.0)),
            icon_sq,
            Space::new().width(Length::Fixed(8.0)),
            info,
        ]
        .align_y(Alignment::Center)
    )
    .width(Length::Fill)
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::ToggleExpand(idx));

    let header = row![
        grip,
        content_btn,
        actions
    ]
    .align_y(Alignment::Center)
    .padding([6, 8]);

    let blend_options = vec!["Normal".to_string(), "Multiply".to_string(), "Screen".to_string()];
    let blend_dropdown = crate::components::dropdown::dropdown(
        blend_options,
        Some("Normal".to_string()),
        |_| Msg::NoOp,
    );

    let opacity_input = crate::components::input::custom_input("100%", "100%", |_| Msg::NoOp);

    let controls = column![
        row![
            blend_dropdown,
            Space::new().width(Length::Fixed(8.0)),
            container(opacity_input).width(Length::Fixed(60.0)),
        ],
        Space::new().height(Length::Fixed(16.0)),
        crate::components::slider::slider("Amount", 65.0, 0.0..=100.0, |_| Msg::SetBlur(0.0)).value_format(|v| format!("{:.0}%", v)),
        Space::new().height(Length::Fixed(12.0)),
        crate::components::slider::slider("Radius", 1.4, 0.0..=5.0, |_| Msg::SetBlur(0.0)).value_format(|v| format!("{:.1} px", v)),
        Space::new().height(Length::Fixed(12.0)),
        crate::components::slider::slider("Threshold", 0.0, 0.0..=255.0, |_| Msg::SetBlur(0.0)).value_format(|v| format!("{:.0}", v)),
        Space::new().height(Length::Fixed(20.0)),
        row![
            crate::components::button::button("Mask")
                .variant(crate::components::button::ButtonVariant::Ghost)
                .width(Length::Fill)
                .on_press(Msg::NoOp),
            Space::new().width(Length::Fixed(6.0)),
            crate::components::button::button("Reset")
                .variant(crate::components::button::ButtonVariant::Ghost)
                .width(Length::Fill)
                .on_press(Msg::NoOp),
            Space::new().width(Length::Fixed(6.0)),
            crate::components::button::button("Presets")
                .variant(crate::components::button::ButtonVariant::Ghost)
                .width(Length::Fill)
                .on_press(Msg::NoOp),
        ]
    ]
    .padding(iced::Padding {
        top: 0.0,
        right: 8.0,
        bottom: 12.0,
        left: 8.0,
    });

    container(column![header, controls])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_ELEVATED)),
            border: Border::default().rounded(4),
            ..Default::default()
        })
        .into()
}

fn build_disabled_filter<'a>(
    idx: usize,
    num: &str,
    title: &str,
    subtitle: &str,
    color: Color,
) -> Element<'a, Msg> {
    let icon_sq = container(
        Space::new()
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0)),
    )
    .style(move |_| container::Style {
        background: Some(Background::Color(color)),
        border: Border::default().rounded(3),
        ..Default::default()
    });

    let info = column![
        text(title.to_string()).size(11).color(TEXT_MUTED),
        text(subtitle.to_string()).size(9).color(TEXT_MUTED),
    ]
    .spacing(2);

    let actions = row![
        crate::components::icon_button::icon_button(INFO).size(12).on_press(Msg::NoOp),
        crate::components::icon_button::icon_button(EYE_OFF).size(12).on_press(Msg::NoOp),
        crate::components::icon_button::icon_button(TRASH).size(12).on_press(Msg::NoOp),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let grip = mouse_area(
        container(text(GRIP_VERTICAL).font(LUCIDE).size(12).color(TEXT_MUTED))
            .padding([4, 4])
    )
    .on_press(Msg::DragStart(idx));

    let content_btn = button(
        row![
            Space::new().width(Length::Fixed(4.0)),
            text(num.to_string()).size(9).color(TEXT_MUTED).font(iced::Font {
                family: iced::font::Family::Monospace,
                ..Default::default()
            }),
            Space::new().width(Length::Fixed(8.0)),
            icon_sq,
            Space::new().width(Length::Fixed(8.0)),
            info,
        ]
        .align_y(Alignment::Center)
    )
    .width(Length::Fill)
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::ToggleExpand(idx));

    container(
        row![
            grip,
            content_btn,
            actions
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 8])
    .style(|_| container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border::default().rounded(4),
        ..Default::default()
    })
    .into()
}


