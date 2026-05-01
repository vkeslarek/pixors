use iced::widget::{container, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::ui::theme::{
    ACCENT, BG_SURFACE, BORDER_SUBTLE, OK_GREEN, STATUSBAR_H, TEXT_MUTED,
    TEXT_SECONDARY,
};
use crate::ui::components::toolbar::Tool;

#[derive(Debug, Clone)]
pub struct State {
    pub connected: bool,
    pub active_tool: Tool,
    pub canvas_w: u32,
    pub canvas_h: u32,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub zoom: f32,
    pub layers: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            connected: false,
            active_tool: Tool::Move,
            canvas_w: 2048,
            canvas_h: 1536,
            mouse_x: 0,
            mouse_y: 0,
            zoom: 100.0,
            layers: 0,
        }
    }
}

impl State {
    pub fn view<Msg: 'static>(&self) -> Element<'_, Msg> {
        fn item<'a, Msg: 'a>(
            label: &'a str,
            value_color: Option<Color>,
            value: String,
        ) -> Element<'a, Msg> {
            row![
                text(label.to_string()).size(10).color(TEXT_MUTED),
                text(value).size(10).color(value_color.unwrap_or(TEXT_SECONDARY)),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
        }

        fn sep<'a, Msg: 'a>() -> Element<'a, Msg> {
            container(text(""))
                .width(1)
                .height(12)
                .style(|_| container::Style {
                    background: Some(Background::Color(BORDER_SUBTLE)),
                    ..Default::default()
                })
                .into()
        }

        let dot_color = if self.connected { OK_GREEN } else { TEXT_MUTED };
        let conn = row![
            container(text(""))
                .width(8)
                .height(8)
                .style(move |_| container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border::default().rounded(999),
                    ..Default::default()
                }),
            text(if self.connected { "Connected" } else { "Disconnected" })
                .size(10)
                .color(TEXT_MUTED),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let canvas_str = if self.canvas_w == 0 {
            "—".to_string()
        } else {
            format!("{}×{}px", self.canvas_w, self.canvas_h)
        };

        let left = row![
            item("Tool:", Some(ACCENT), self.active_tool.label().to_string()),
            sep(),
            item("Canvas:", None, canvas_str),
            sep(),
            row![
                item("X:", Some(ACCENT), self.mouse_x.to_string()),
                item("Y:", Some(ACCENT), self.mouse_y.to_string()),
            ]
            .spacing(8),
            sep(),
            item("Zoom:", Some(ACCENT), format!("{:.0}%", self.zoom)),
            sep(),
            item("Layers:", Some(ACCENT), self.layers.to_string()),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        let right = row![
            conn,
            sep(),
            text("RGB/8").size(10).color(TEXT_MUTED),
            sep(),
            text("sRGB").size(10).color(TEXT_MUTED),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        container(
            row![left, iced::widget::horizontal_space(), right]
                .spacing(12)
                .padding([0, 12])
                .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(STATUSBAR_H)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_SURFACE)),
            border: Border {
                width: 0.0,
                color: BORDER_SUBTLE,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }
}
