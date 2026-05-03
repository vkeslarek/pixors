use iced::border::Radius;
use iced::widget::{button, container, mouse_area, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::ui::theme::{
    ACCENT, BG_BASE, BG_HOVER, BG_SURFACE, BORDER, BORDER_SUBTLE,
    TABBAR_H, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

#[derive(Debug, Clone)]
pub enum Msg {
    Select(usize),
    Add,
    Close(usize),
    DragStart(usize),
    DragHover(usize),
    DragDrop,
}

#[derive(Debug, Clone)]
pub struct State {
    pub tabs: Vec<String>,
    pub active: usize,
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tabs: vec!["Untitled 1".into(), "Untitled 2".into(), "Untitled 3".into()],
            active: 0,
            drag_from: None,
            drag_over: None,
        }
    }
}

impl State {
    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Select(i) => self.active = i,
            Msg::Add => {
                self.tabs.push(format!("Untitled {}", self.tabs.len() + 1));
                self.active = self.tabs.len() - 1;
            }
            Msg::Close(i) => {
                if self.tabs.len() > 1 {
                    self.tabs.remove(i);
                    if self.active >= self.tabs.len() {
                        self.active = self.tabs.len() - 1;
                    }
                }
            }
            Msg::DragStart(i) => {
                self.active = i;
                self.drag_from = Some(i);
                self.drag_over = Some(i);
            }
            Msg::DragHover(i) => {
                if self.drag_from.is_some() {
                    self.drag_over = Some(i);
                }
            }
            Msg::DragDrop => {
                if let (Some(from), Some(to)) = (self.drag_from, self.drag_over)
                    && from != to
                {
                    self.tabs.swap(from, to);
                    if self.active == from {
                        self.active = to;
                    } else if self.active == to {
                        self.active = from;
                    }
                }
                self.drag_from = None;
                self.drag_over = None;
            }
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let colors = [
            Color::from_rgb(1.0, 0.30, 0.30),
            Color::from_rgb(0.30, 1.0, 0.30),
            Color::from_rgb(0.30, 0.30, 1.0),
            Color::from_rgb(1.0, 1.0, 0.30),
            Color::from_rgb(1.0, 0.30, 1.0),
            Color::from_rgb(0.40, 1.0, 1.0),
        ];

        let mut all: Vec<Element<Msg>> = Vec::new();
        for (i, name) in self.tabs.iter().enumerate() {
            all.push(tab_view(
                i,
                name,
                i == self.active,
                self.drag_from,
                self.drag_over,
                colors[i % colors.len()],
            ));
        }

        let row = row(all)
            .spacing(2)
            .padding([0, 8])
            .align_y(iced::Alignment::End);

        container(
            mouse_area(row).on_release(Msg::DragDrop),
        )
        .width(Length::Fill)
        .height(TABBAR_H)
        .align_y(iced::alignment::Vertical::Bottom)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_BASE)),
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

fn tab_view<'a>(
    i: usize,
    name: &'a str,
    is_active: bool,
    drag_from: Option<usize>,
    drag_over: Option<usize>,
    _dot_color: Color,
) -> Element<'a, Msg> {
    let is_dragged = drag_from == Some(i);
    let is_hover_target = drag_over == Some(i) && drag_from.is_some_and(|f| f != i);

    let label = text(name)
        .size(12)
        .color(if is_active { TEXT_PRIMARY } else { TEXT_MUTED });

    let close = button(
        text(crate::ui::icons::X)
            .size(12)
            .font(crate::ui::icons::LUCIDE)
            .color(TEXT_SECONDARY)
            .center(),
    )
    .on_press(Msg::Close(i))
    .padding(0)
    .width(16)
    .height(16)
    .style(|_, status| {
        let bg = if matches!(status, button::Status::Hovered) {
            Color::from_rgba(1.0, 1.0, 1.0, 0.1)
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            border: Border::default().rounded(3),
            text_color: TEXT_SECONDARY,
            ..Default::default()
        }
    });

    let inner = container(
        row![label, close]
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .padding([6, 16]);

    let btn = button(inner)
        .padding(0)
        .width(Length::Shrink)
        .style(move |_, status| {
            let is_hovered = matches!(status, button::Status::Hovered);

            let bg = if is_hover_target {
                Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.30)
            } else if is_dragged {
                Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.10)
            } else if is_active {
                crate::ui::theme::BG_ACTIVE
            } else if is_hovered {
                crate::ui::theme::BG_HOVER
            } else {
                Color::TRANSPARENT
            };

            let current_border_color = if is_hover_target {
                ACCENT
            } else if is_active {
                BORDER_SUBTLE
            } else {
                Color::TRANSPARENT
            };

            let current_border_width = if is_hover_target { 2.0 } else if is_active { 1.0 } else { 0.0 };

            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    width: current_border_width,
                    color: current_border_color,
                    radius: Radius {
                        top_left: 6.0,
                        top_right: 6.0,
                        bottom_right: 0.0,
                        bottom_left: 0.0,
                    },
                },
                text_color: TEXT_SECONDARY,
                ..Default::default()
            }
        });

    mouse_area(btn)
        .on_press(Msg::DragStart(i))
        .on_enter(Msg::DragHover(i))
        .into()
}
