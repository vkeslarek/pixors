use iced::widget::{column, container, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::{ACCENT, BG_ELEVATED, TEXT_MUTED, TEXT_SECONDARY};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Msg {
    Close,
    Select(usize),
    ToggleVisibility(usize),
}

pub fn view<'a>(layers: &'a [pixors_state::tab::Layer], active_idx: usize) -> Element<'a, Msg> {
    if layers.is_empty() {
        container(text("No layers yet.").size(12).color(TEXT_MUTED))
            .padding(16)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into()
    } else {
        let items: Vec<Element<Msg>> = layers
            .iter()
            .enumerate()
            .map(|(i, layer)| layer_row(i, layer, i == active_idx))
            .collect();

        column(items)
            .spacing(2)
            .padding([4, 8])
            .width(Length::Fill)
            .into()
    }
}

fn layer_row<'a>(
    i: usize,
    layer: &'a pixors_state::tab::Layer,
    is_active: bool,
) -> Element<'a, Msg> {
    let color = match &layer.source {
        pixors_state::tab::LayerSource::FilePage { .. } => Color::from_rgb(0.3, 0.5, 0.8),
        pixors_state::tab::LayerSource::SolidColor { color } => Color::from_rgba(
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            1.0,
        ),
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

    let name = text(layer.name.as_str()).size(11).color(TEXT_SECONDARY);

    let eye_icon = if layer.visible {
        crate::icons::EYE
    } else {
        crate::icons::EYE_OFF
    };
    let visibility_btn = crate::components::icon_button::icon_button(eye_icon)
        .size(12)
        .on_press(Msg::ToggleVisibility(i));

    let opacity_text = text(format!("{}%", (layer.opacity * 100.0) as u32))
        .size(9)
        .color(TEXT_MUTED);

    let row_content = row![
        thumb,
        name,
        iced::widget::Space::new().width(Length::Fill),
        opacity_text,
        visibility_btn
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

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
        })
        .into()
}
