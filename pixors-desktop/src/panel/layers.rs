use iced::widget::{column, container, mouse_area, row, slider, text};
use iced::{Alignment, Background, Border, Color, Element, Length};
use pixors_document::SessionId;
use pixors_document::document::{LayerNode, NodeId};

use crate::effect::Effect;
use crate::theme::{ACCENT, BG_ELEVATED, TEXT_MUTED, TEXT_SECONDARY};

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    Select(NodeId),
    ToggleVisibility(NodeId),
    SetOpacityPreview(NodeId, f32),
    SetOpacityCommit(NodeId),
    DragStart(usize),
    DragHover(usize),
    DragDrop,
}

#[derive(Debug, Clone, Default)]
pub struct LayersPanelState {
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
    pub pending_opacity: Option<(NodeId, f32)>,
}

impl LayersPanelState {
    pub fn update(&mut self, msg: &Msg) {
        match msg {
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
            Msg::SetOpacityPreview(id, opacity) => {
                self.pending_opacity = Some((*id, *opacity));
            }
            _ => {}
        }
    }
}

pub struct LayersContext<'a> {
    pub active_tab_id: Option<SessionId>,
    pub layers: &'a [LayerNode],
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
}

pub fn update(msg: Msg, ctx: LayersContext<'_>) -> Vec<Effect> {
    let Some(session_id) = ctx.active_tab_id else {
        return vec![];
    };
    match msg {
        Msg::Close => vec![Effect::TogglePane(crate::app::PaneKind::Layers)],
        Msg::Select(id) => {
            // Direct state mutation — not a Document mutation
            vec![Effect::SelectLayer {
                session_id,
                layer_id: id,
            }]
        }
        Msg::ToggleVisibility(id) => {
            let visible = ctx
                .layers
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.visible)
                .unwrap_or(true);
            vec![
                Effect::Commit(std::sync::Arc::new(
                    pixors_document::mutation::impls::SetLayerVisible {
                        tab: session_id,
                        layer: id,
                        before: visible,
                        after: !visible,
                    },
                )),
                Effect::QueueDisplayRefresh(session_id),
            ]
        }
        Msg::SetOpacityPreview(_, _) => vec![],
        Msg::SetOpacityCommit(id) => {
            let before = ctx
                .layers
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.blend.opacity)
                .unwrap_or(1.0);
            vec![
                Effect::Commit(std::sync::Arc::new(
                    pixors_document::mutation::impls::SetLayerOpacity {
                        tab: session_id,
                        layer: id,
                        before,
                        after: before, // will be filled by controller from pending_opacity
                    },
                )),
                Effect::QueueDisplayRefresh(session_id),
            ]
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
            if from >= ctx.layers.len() || to >= ctx.layers.len() {
                return vec![];
            }
            vec![
                Effect::Commit(std::sync::Arc::new(
                    pixors_document::mutation::impls::SwapLayers {
                        tab: session_id,
                        index_a: from,
                        index_b: to,
                    },
                )),
                Effect::QueueDisplayRefresh(session_id),
            ]
        }
        Msg::DragStart(_) | Msg::DragHover(_) => vec![],
    }
}

pub fn view_slice<'a>(
    layers: &'a [LayerNode],
    active_id: Option<NodeId>,
    state: &'a LayersPanelState,
) -> Element<'a, Msg> {
    if layers.is_empty() {
        container(text("No layers yet.").size(12).color(TEXT_MUTED))
            .padding(16)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into()
    } else {
        let mut elements = Vec::new();
        for (idx, l) in layers.iter().enumerate() {
            let is_active = active_id == Some(l.id);
            let is_dragged = state.drag_from == Some(idx);
            let is_hover_target =
                state.drag_over == Some(idx) && state.drag_from.is_some_and(|from| from != idx);

            let row_el = layer_row(l, idx, is_active, state);
            let wrapper = container(row_el).style(move |_| {
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

            let area = mouse_area(wrapper).on_enter(Msg::DragHover(idx));
            elements.push(area.into());
        }

        mouse_area(column(elements).spacing(2).padding([4, 8]))
            .on_release(Msg::DragDrop)
            .into()
    }
}

fn layer_row<'a>(layer: &'a LayerNode, index: usize, is_active: bool, state: &'a LayersPanelState) -> Element<'a, Msg> {
    let color = match &layer.source {
        pixors_document::PixelSource::PrimaryAsset { .. } => Color::from_rgb(0.3, 0.5, 0.8),
        pixors_document::PixelSource::SolidColor { .. } => Color::from_rgba(0.6, 0.6, 0.2, 1.0),
    };

    let grip = mouse_area(
        container(
            text(crate::icons::GRIP_VERTICAL)
                .font(crate::icons::LUCIDE)
                .size(12)
                .color(TEXT_MUTED),
        )
        .padding([4, 4]),
    )
    .on_press(Msg::DragStart(index));

    let thumb = container(text(""))
        .width(28)
        .height(28)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: 3.0.into(),
                width: if is_active { 2.0 } else { 0.0 },
                color: if is_active {
                    ACCENT
                } else {
                    Color::TRANSPARENT
                },
            },
            ..Default::default()
        });

    let eye_icon = if layer.visible {
        crate::icons::EYE
    } else {
        crate::icons::EYE_OFF
    };
    let visibility_btn = crate::components::icon_button::icon_button(eye_icon)
        .size(12)
        .on_press(Msg::ToggleVisibility(layer.id));

    let current_opacity = state
        .pending_opacity
        .and_then(|(pid, o)| if pid == layer.id { Some(o) } else { None })
        .unwrap_or(layer.blend.opacity);

    let opacity_slider = slider(0.0..=1.0, current_opacity, |v| {
        Msg::SetOpacityPreview(layer.id, v)
    })
    .width(60)
    .step(0.01)
    .on_release(Msg::SetOpacityCommit(layer.id));

    let opacity_label = text(format!("{}%", (current_opacity * 100.0) as u32))
        .size(9)
        .color(TEXT_MUTED);

    let name_label =
        container(text(layer.name.as_str()).size(11).color(TEXT_SECONDARY)).width(Length::Fill);

    let row_content = row![
        grip,
        thumb,
        name_label,
        opacity_label,
        opacity_slider,
        visibility_btn,
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let layer_id = layer.id;

    mouse_area(
        container(row_content)
            .padding([6, 8])
            .style(move |_| container::Style {
                background: Some(Background::Color(if is_active {
                    BG_ELEVATED
                } else {
                    Color::TRANSPARENT
                })),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_press(Msg::Select(layer_id))
    .into()
}
