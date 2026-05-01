use iced::widget::{column, container, text};
use iced::{Background, Border, Element, Length};

use crate::ui::theme::{BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED};
use crate::ui::widgets::panel_header;

#[derive(Debug, Clone)]
pub enum Msg {
    Close,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub layers: Vec<LayerInfo>,
}

#[derive(Debug, Clone)]
pub struct LayerInfo {
    pub name: String,
}

impl State {
    pub fn update(&mut self, _msg: Msg) {}

    pub fn body_view(&self) -> Element<'_, Msg> {
        if self.layers.is_empty() {
            container(text("No layers yet.").size(12).color(TEXT_MUTED))
                .padding(16)
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into()
        } else {
            column(self.layers.iter().map(|l| {
                container(text(l.name.as_str()).size(12).color(TEXT_MUTED))
                    .padding(8)
                    .width(Length::Fill)
                    .into()
            }))
            .into()
        }
    }

    #[allow(dead_code)]
    pub fn view(&self) -> Element<'_, Msg> {
        let header = panel_header::<Msg>("Layers", Some(Msg::Close));
        container(column![header, self.body_view()].spacing(0))
            .width(Length::Fill)
            .height(Length::Fill)
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
