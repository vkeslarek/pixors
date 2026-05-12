use std::collections::HashSet;

use iced::widget::{Space, button, column, container, mouse_area, row, scrollable, slider, text};
use iced::{Alignment, Background, Border, Color, Element, Length};
use pixors_document::{NodeId, SessionId, Transform};

use crate::effect::Effect;

use crate::icons::{EYE, EYE_OFF, GRIP_VERTICAL, LUCIDE, PLUS, TRASH};
use crate::theme::{
    ACCENT, BG_BASE, BG_ELEVATED, BG_HOVER, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    ToggleExpand(usize),
    ToggleEnabled(NodeId),
    RemoveFilter(NodeId),
    SetBlur(f32),
    CommitBlur(f32),
    CancelPreview,
    DragStart(usize),
    DragHover(usize),
    DragDrop,
    OpenFilterSearch,
    NoOp,
}

#[derive(Debug, Clone, Default)]
pub struct FilterPanelState {
    pub expanded: HashSet<usize>,
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
    pub dragging_radius: Option<f32>,
}

impl FilterPanelState {
    pub fn update(&mut self, msg: &Msg) {
        match msg {
            Msg::ToggleExpand(idx) => {
                if self.expanded.contains(idx) {
                    self.expanded.remove(idx);
                } else {
                    self.expanded.insert(*idx);
                }
            }
            Msg::DragStart(idx) => {
                self.drag_from = Some(*idx);
                self.drag_over = Some(*idx);
            }
            Msg::DragHover(idx) => {
                if self.drag_from.is_some() {
                    self.drag_over = Some(*idx);
                }
            }
            Msg::DragDrop => {
                self.drag_from = None;
                self.drag_over = None;
            }
            Msg::SetBlur(v) => {
                self.dragging_radius = Some(*v);
            }
            Msg::CancelPreview => {
                self.dragging_radius = None;
            }
            _ => {}
        }
    }
}

pub struct FilterContext {
    pub session_id: SessionId,
    pub active_layer_id: Option<NodeId>,
    pub transforms: Vec<Transform>,
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
}

impl FilterContext {
    pub fn new(
        session_id: SessionId,
        active_layer_id: Option<NodeId>,
        transforms: &[Transform],
        drag_from: Option<usize>,
        drag_over: Option<usize>,
    ) -> Self {
        Self {
            session_id,
            active_layer_id,
            transforms: transforms.to_vec(),
            drag_from,
            drag_over,
        }
    }
}

pub fn update(msg: Msg, ctx: FilterContext) -> Vec<Effect> {
    match msg {
        Msg::Close => vec![Effect::TogglePane(crate::app::PaneKind::Filters)],
        Msg::OpenFilterSearch => vec![Effect::ShowFilterSearch],
        Msg::ToggleEnabled(transform_id) => {
            let Some(layer_id) = ctx.active_layer_id else {
                return vec![];
            };
            if let Some(t) = ctx.transforms.iter().find(|t| t.id == transform_id) {
                vec![
                    Effect::Commit(std::sync::Arc::new(
                        pixors_document::mutation::impls::SetTransformEnabled {
                            tab: ctx.session_id,
                            layer: layer_id,
                            transform_id: t.id,
                            before: t.enabled,
                            after: !t.enabled,
                        },
                    )),
                    Effect::QueueDisplayRefresh(ctx.session_id),
                ]
            } else {
                vec![]
            }
        }
        Msg::RemoveFilter(transform_id) => {
            let Some(layer_id) = ctx.active_layer_id else {
                return vec![];
            };
            if let Some((idx, removed)) = ctx
                .transforms
                .iter()
                .enumerate()
                .find(|(_, t)| t.id == transform_id)
                .map(|(i, t)| (i, t.clone()))
            {
                vec![
                    Effect::Commit(std::sync::Arc::new(
                        pixors_document::mutation::impls::RemoveTransform {
                            tab: ctx.session_id,
                            layer: layer_id,
                            transform_id,
                            removed,
                            index: idx,
                        },
                    )),
                    Effect::QueueDisplayRefresh(ctx.session_id),
                ]
            } else {
                vec![]
            }
        }
        Msg::DragDrop => {
            let Some(from) = ctx.drag_from else {
                return vec![];
            };
            let Some(to) = ctx.drag_over else {
                return vec![];
            };
            if from == to {
                return vec![];
            }
            let Some(layer_id) = ctx.active_layer_id else {
                return vec![];
            };
            if from >= ctx.transforms.len() || to >= ctx.transforms.len() {
                return vec![];
            }
            vec![Effect::Commit(std::sync::Arc::new(
                pixors_document::mutation::impls::ReorderTransform {
                    tab: ctx.session_id,
                    layer: layer_id,
                    from,
                    to,
                },
            ))]
        }
        // State-only messages handled by FilterPanelState::update()
        Msg::ToggleExpand(_) | Msg::DragStart(_) | Msg::DragHover(_) | Msg::NoOp => vec![],
        // Blur preview/commit/cancel handled by controller directly (needs graph building)
        Msg::SetBlur(_) | Msg::CommitBlur(_) | Msg::CancelPreview => vec![],
    }
}

pub fn view<'a>(
    transforms: &'a [Transform],
    blur_preview_radius: Option<f32>,
    state: &'a FilterPanelState,
) -> Element<'a, Msg> {
    let toolbar = build_toolbar();
    let filter_rows = build_filter_rows(transforms, blur_preview_radius, state);
    let content = column![toolbar, filter_rows].spacing(0);

    let footer = build_footer(transforms);

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

fn build_toolbar<'a>() -> Element<'a, Msg> {
    container(
        button(
            row![
                text(PLUS).font(LUCIDE).size(14).color(TEXT_SECONDARY),
                text("Add filter").size(13).color(TEXT_SECONDARY),
                Space::new().width(Length::Fill),
                container(text("\u{2318}F").size(9).color(TEXT_MUTED))
                    .padding([2, 4])
                    .style(|_| container::Style {
                        background: Some(Background::Color(BG_BASE)),
                        border: Border::default().rounded(4),
                        ..Default::default()
                    }),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([8, 12])
        .style(|_theme, state| {
            let (bg, text_color) = match state {
                iced::widget::button::Status::Hovered => (Some(BG_HOVER), TEXT_PRIMARY),
                _ => (Some(BG_SURFACE), TEXT_PRIMARY),
            };
            iced::widget::button::Style {
                background: bg.map(Background::Color),
                text_color,
                border: Border {
                    color: crate::theme::BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(Msg::OpenFilterSearch),
    )
    .padding(iced::Padding {
        top: 12.0,
        right: 16.0,
        bottom: 12.0,
        left: 16.0,
    })
    .into()
}

fn build_filter_rows<'a>(
    transforms: &'a [Transform],
    blur_preview_radius: Option<f32>,
    state: &'a FilterPanelState,
) -> Element<'a, Msg> {
    let mut elements = Vec::new();

    for (i, t) in transforms.iter().enumerate() {
        let num = format!("{:02}", i + 1);
        let is_dragged = state.drag_from == Some(i);
        let is_hover_target =
            state.drag_over == Some(i) && state.drag_from.is_some_and(|from| from != i);
        let is_expanded = state.expanded.contains(&i);

        let el = if !t.enabled {
            build_disabled_filter_row(i, &num, t, blur_preview_radius, is_expanded)
        } else if is_expanded {
            build_expanded_filter_row(i, &num, t, blur_preview_radius)
        } else {
            build_collapsed_filter_row(i, &num, t, blur_preview_radius)
        };

        let wrapper = container(el).style(move |_| {
            if is_hover_target {
                container::Style {
                    background: Some(Background::Color(Color::from_rgba(
                        ACCENT.r, ACCENT.g, ACCENT.b, 0.30,
                    ))),
                    ..Default::default()
                }
            } else if is_dragged {
                container::Style {
                    background: Some(Background::Color(Color::from_rgba(
                        ACCENT.r, ACCENT.g, ACCENT.b, 0.10,
                    ))),
                    ..Default::default()
                }
            } else {
                container::Style::default()
            }
        });

        let area = mouse_area(wrapper).on_enter(Msg::DragHover(i));
        elements.push(area.into());
    }

    mouse_area(column(elements).spacing(2).padding([4, 8]))
        .on_release(Msg::DragDrop)
        .into()
}

fn transform_color(t: &Transform) -> Color {
    use pixors_document::Operation;
    match &t.op {
        Operation::Blur { .. } => Color::from_rgb(0.5, 0.4, 0.7),
        Operation::Exposure { .. } => Color::from_rgb(0.8, 0.7, 0.3),
    }
}

fn transform_subtitle(t: &Transform) -> String {
    use pixors_document::Operation;
    match &t.op {
        Operation::Blur { radius } => format!("radius {:.0}px", radius),
        Operation::Exposure { stops } => format!("{:+.1} stops", stops),
    }
}

fn build_collapsed_filter_row<'a>(
    idx: usize,
    num: &str,
    t: &'a Transform,
    _blur_preview_radius: Option<f32>,
) -> Element<'a, Msg> {
    let color = transform_color(t);
    let subtitle = transform_subtitle(t);
    let title = t.op.label().to_string();
    let opacity = match &t.output {
        pixors_document::OutputMode::Replace { blend }
        | pixors_document::OutputMode::Composite { blend, .. } => {
            format!("{}%", (blend.opacity * 100.0) as u32)
        }
    };

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
        text(title).size(11).color(TEXT_SECONDARY),
        row![
            text(subtitle).size(9).color(TEXT_MUTED),
            text(opacity).size(9).color(ACCENT),
        ]
    ]
    .spacing(2);

    let actions = row![
        crate::components::icon_button::icon_button(EYE)
            .size(12)
            .on_press(Msg::ToggleEnabled(t.id)),
        crate::components::icon_button::icon_button(TRASH)
            .size(12)
            .on_press(Msg::RemoveFilter(t.id)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let grip = mouse_area(
        container(text(GRIP_VERTICAL).font(LUCIDE).size(12).color(TEXT_MUTED)).padding([4, 4]),
    )
    .on_press(Msg::DragStart(idx));

    let content_btn = button(
        row![
            Space::new().width(Length::Fixed(4.0)),
            text(num.to_string())
                .size(9)
                .color(TEXT_MUTED)
                .font(iced::Font {
                    family: iced::font::Family::Monospace,
                    ..Default::default()
                }),
            Space::new().width(Length::Fixed(8.0)),
            icon_sq,
            Space::new().width(Length::Fixed(8.0)),
            info,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::ToggleExpand(idx));

    container(row![grip, content_btn, actions].align_y(Alignment::Center))
        .padding([6, 8])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default().rounded(4),
            ..Default::default()
        })
        .into()
}

fn build_expanded_filter_row<'a>(
    idx: usize,
    num: &str,
    t: &'a Transform,
    blur_preview_radius: Option<f32>,
) -> Element<'a, Msg> {
    let color = transform_color(t);
    let subtitle = transform_subtitle(t);
    let title = t.op.label().to_string();
    let _opacity1 = match &t.output {
        pixors_document::OutputMode::Replace { blend }
        | pixors_document::OutputMode::Composite { blend, .. } => blend.opacity,
    };

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
        text(title).size(11).color(TEXT_PRIMARY).font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        }),
        row![text(subtitle).size(9).color(TEXT_MUTED)]
    ]
    .spacing(2);

    let actions = row![
        crate::components::icon_button::icon_button(EYE)
            .size(12)
            .on_press(Msg::ToggleEnabled(t.id)),
        crate::components::icon_button::icon_button(TRASH)
            .size(12)
            .on_press(Msg::RemoveFilter(t.id)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let grip = mouse_area(
        container(text(GRIP_VERTICAL).font(LUCIDE).size(12).color(TEXT_MUTED)).padding([4, 4]),
    )
    .on_press(Msg::DragStart(idx));

    let content_btn = button(
        row![
            Space::new().width(Length::Fixed(4.0)),
            text(num.to_string())
                .size(9)
                .color(ACCENT)
                .font(iced::Font {
                    family: iced::font::Family::Monospace,
                    ..Default::default()
                }),
            Space::new().width(Length::Fixed(8.0)),
            icon_sq,
            Space::new().width(Length::Fixed(8.0)),
            info,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::ToggleExpand(idx));

    let header = row![grip, content_btn, actions]
        .align_y(Alignment::Center)
        .padding([6, 8]);

    let controls = build_filter_controls(t, blur_preview_radius);

    container(column![header, controls])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_ELEVATED)),
            border: Border::default().rounded(4),
            ..Default::default()
        })
        .into()
}

fn build_filter_controls<'a>(
    t: &'a Transform,
    blur_preview_radius: Option<f32>,
) -> Element<'a, Msg> {
    use pixors_document::Operation;

    let _opacity = match &t.output {
        pixors_document::OutputMode::Replace { blend }
        | pixors_document::OutputMode::Composite { blend, .. } => blend.opacity,
    };

    let mut controls: Vec<Element<Msg>> = Vec::new();

    match &t.op {
        Operation::Blur { radius } => {
            let r = blur_preview_radius.unwrap_or(*radius);
            controls.push(
                column![
                    text(format!("Radius: {:.1} px", r))
                        .size(11)
                        .color(TEXT_SECONDARY),
                    slider(1.0..=64.0, r, Msg::SetBlur)
                        .width(Length::Fill)
                        .step(0.5)
                        .on_release(Msg::CommitBlur(r)),
                ]
                .spacing(4)
                .into(),
            );
        }
        Operation::Exposure { .. } => {
            controls.push(
                text("Exposure controls coming soon")
                    .size(11)
                    .color(TEXT_MUTED)
                    .into(),
            );
        }
    }

    controls.push(Space::new().height(Length::Fixed(12.0)).into());

    let row_ctrls = row![
        crate::components::button("Reset")
            .variant(crate::components::button::ButtonVariant::Ghost)
            .width(Length::Fill)
            .on_press(Msg::CancelPreview),
        Space::new().width(Length::Fixed(6.0)),
        crate::components::button("Remove")
            .variant(crate::components::button::ButtonVariant::Danger)
            .width(Length::Fill)
            .on_press(Msg::RemoveFilter(t.id)),
    ];
    controls.push(row_ctrls.into());

    column(controls)
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 12.0,
            left: 8.0,
        })
        .into()
}

fn build_disabled_filter_row<'a>(
    idx: usize,
    num: &str,
    t: &'a Transform,
    _blur_preview_radius: Option<f32>,
    _is_expanded: bool,
) -> Element<'a, Msg> {
    let color = Color::from_rgba(
        transform_color(t).r * 0.5,
        transform_color(t).g * 0.5,
        transform_color(t).b * 0.5,
        0.6,
    );
    let subtitle = transform_subtitle(t);
    let title = t.op.label().to_string();

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
        text(title).size(11).color(TEXT_MUTED),
        text(subtitle).size(9).color(TEXT_MUTED),
    ]
    .spacing(2);

    let actions = row![
        crate::components::icon_button::icon_button(EYE_OFF)
            .size(12)
            .on_press(Msg::ToggleEnabled(t.id)),
        crate::components::icon_button::icon_button(TRASH)
            .size(12)
            .on_press(Msg::RemoveFilter(t.id)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let grip = mouse_area(
        container(text(GRIP_VERTICAL).font(LUCIDE).size(12).color(TEXT_MUTED)).padding([4, 4]),
    )
    .on_press(Msg::DragStart(idx));

    let content_btn = button(
        row![
            Space::new().width(Length::Fixed(4.0)),
            text(num.to_string())
                .size(9)
                .color(TEXT_MUTED)
                .font(iced::Font {
                    family: iced::font::Family::Monospace,
                    ..Default::default()
                }),
            Space::new().width(Length::Fixed(8.0)),
            icon_sq,
            Space::new().width(Length::Fixed(8.0)),
            info,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(0)
    .style(|_, _| button::Style::default())
    .on_press(Msg::ToggleExpand(idx));

    container(row![grip, content_btn, actions].align_y(Alignment::Center))
        .padding([6, 8])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default().rounded(4),
            ..Default::default()
        })
        .into()
}

fn build_footer<'a>(transforms: &'a [Transform]) -> Element<'a, Msg> {
    let active = transforms.iter().filter(|t| t.enabled).count();

    container(column![
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
            text(format!("{} active", active))
                .size(11)
                .color(TEXT_SECONDARY),
        ]
        .align_y(Alignment::Center),
    ])
    .padding([12, 16])
    .style(|_| container::Style {
        border: Border {
            width: 1.0,
            color: BORDER_SUBTLE,
            ..Border::default()
        },
        background: Some(Background::Color(BG_SURFACE)),
        ..Default::default()
    })
    .into()
}
