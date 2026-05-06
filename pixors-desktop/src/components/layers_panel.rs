use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::{ACCENT, BG_ELEVATED, TEXT_MUTED, TEXT_SECONDARY};

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
    Select(usize),
    ToggleVisibility(usize),
}

#[derive(Debug, Clone)]
pub struct LayerInfo {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub color: Color,
}

impl LayerInfo {
    fn fake_layers() -> Vec<Self> {
        vec![
            LayerInfo { name: "Background".into(), visible: true, opacity: 1.0, color: Color::from_rgb(0.3, 0.5, 0.8) },
            LayerInfo { name: "Gradient Map".into(), visible: true, opacity: 0.8, color: Color::from_rgb(0.8, 0.3, 0.5) },
            LayerInfo { name: "Text overlay".into(), visible: true, opacity: 1.0, color: Color::from_rgb(0.9, 0.9, 0.2) },
            LayerInfo { name: "Vignette".into(), visible: false, opacity: 0.5, color: Color::from_rgb(0.1, 0.1, 0.1) },
            LayerInfo { name: "Sharpening".into(), visible: true, opacity: 1.0, color: Color::from_rgb(0.5, 0.5, 0.5) },
        ]
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub layers: Vec<LayerInfo>,
    pub active: usize,
}

impl Default for State {
    fn default() -> Self {
        let layers = LayerInfo::fake_layers();
        Self { layers, active: 0 }
    }
}

impl State {
    pub fn body_view(&self) -> Element<'_, Msg> {
        if self.layers.is_empty() {
            container(text("No layers yet.").size(12).color(TEXT_MUTED))
                .padding(16)
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into()
        } else {
            let items: Vec<Element<Msg>> = self
                .layers
                .iter()
                .enumerate()
                .map(|(i, layer)| layer_row(i, layer, i == self.active))
                .collect();

            column(items).spacing(2).padding([4, 8]).width(Length::Fill).into()
        }
    }
}

fn layer_row<'a>(i: usize, layer: &'a LayerInfo, is_active: bool) -> Element<'a, Msg> {
    let thumb = container(text(""))
        .width(28)
        .height(28)
        .style(move |_| container::Style {
            background: Some(Background::Color(layer.color)),
            border: Border {
                radius: 3.0.into(),
                width: if is_active { 2.0 } else { 0.0 },
                color: if is_active { ACCENT } else { Color::TRANSPARENT },
            },
            ..Default::default()
        });

    let name = text(layer.name.as_str())
        .size(11)
        .color(TEXT_SECONDARY);

    let eye_icon = if layer.visible { "👁" } else { "—" };
    let visibility_btn = button(text(eye_icon).size(12))
        .on_press(Msg::ToggleVisibility(i))
        .padding(2)
        .style(|_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            let bg = if hovered { Color::from_rgba(1.0, 1.0, 1.0, 0.08) } else { Color::TRANSPARENT };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border::default().rounded(3),
                text_color: TEXT_SECONDARY,
                ..Default::default()
            }
        });

    let opacity_text = text(format!("{}%", (layer.opacity * 100.0) as u32))
        .size(9)
        .color(TEXT_MUTED);

    let row_content = row![thumb, name, iced::widget::Space::new().width(Length::Fill), opacity_text, visibility_btn]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    container(row_content)
        .padding([6, 8])
        .style(move |_| container::Style {
            background: Some(Background::Color(if is_active { BG_ELEVATED } else { Color::TRANSPARENT })),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
