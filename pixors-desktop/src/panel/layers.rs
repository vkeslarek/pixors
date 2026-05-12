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
    SetOpacity(NodeId, f32),
}

pub struct LayersContext<'a> {
    pub active_tab_id: Option<SessionId>,
    pub layers: &'a [LayerNode],
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
        Msg::SetOpacity(id, opacity) => {
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
                        after: opacity,
                    },
                )),
                Effect::QueueDisplayRefresh(session_id),
            ]
        }
    }
}

pub fn view_slice<'a>(layers: &'a [LayerNode], active_id: Option<NodeId>) -> Element<'a, Msg> {
    if layers.is_empty() {
        container(text("No layers yet.").size(12).color(TEXT_MUTED))
            .padding(16)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into()
    } else {
        column(
            layers
                .iter()
                .map(|l| layer_row(l, active_id == Some(l.id)))
                .collect::<Vec<_>>(),
        )
        .spacing(2)
        .padding([4, 8])
        .width(Length::Fill)
        .into()
    }
}

fn layer_row<'a>(layer: &'a LayerNode, is_active: bool) -> Element<'a, Msg> {
    let color = match &layer.source {
        pixors_document::PixelSource::PrimaryAsset { .. } => Color::from_rgb(0.3, 0.5, 0.8),
        pixors_document::PixelSource::SolidColor { .. } => Color::from_rgba(0.6, 0.6, 0.2, 1.0),
    };

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

    let opacity_slider = slider(0.0..=1.0, layer.blend.opacity, |v| {
        Msg::SetOpacity(layer.id, v)
    })
    .width(60)
    .step(0.01);

    let opacity_label = text(format!("{}%", (layer.blend.opacity * 100.0) as u32))
        .size(9)
        .color(TEXT_MUTED);

    let row_content = row![
        thumb,
        text(layer.name.as_str()).size(11).color(TEXT_SECONDARY),
        iced::widget::Space::new().width(Length::Fill),
        opacity_label,
        opacity_slider,
        visibility_btn,
    ]
    .spacing(6)
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
