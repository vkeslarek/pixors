use iced::border::Radius;
use iced::widget::{button, container, mouse_area, row, text};
use iced::{Background, Border, Color, Element, Length};

use pixors_state::state::{EditorState, TabId};
use crate::theme::{
    ACCENT, BG_BASE, BORDER_SUBTLE, TABBAR_H, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

#[derive(Debug, Clone)]
pub enum Msg {
    Select(TabId),
    Close(TabId),
    DragStart(usize),
    DragHover(usize),
    DragDrop,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub drag_from: Option<usize>,
    pub drag_over: Option<usize>,
}

impl State {
    pub fn update(&mut self, msg: Msg, tab_count: usize) {
        match msg {
            Msg::Select(_) | Msg::Close(_) => {}
            Msg::DragStart(i) if i < tab_count => {
                self.drag_from = Some(i);
                self.drag_over = Some(i);
            }
            Msg::DragHover(i) if self.drag_from.is_some() && i < tab_count => {
                self.drag_over = Some(i);
            }
            Msg::DragDrop => {
                self.drag_from = None;
                self.drag_over = None;
            }
            _ => {}
        }
    }

    pub fn view<'a>(&'a self, editor: &'a EditorState) -> Element<'a, Msg> {
        let active_id = editor.active_id();
        let tabs = editor.tabs();

        let mut all: Vec<Element<Msg>> = Vec::new();
        for (i, tab) in tabs.iter().enumerate() {
            let is_active = active_id == Some(tab.id);
            all.push(tab_view(
                i,
                &tab.title,
                is_active,
                tab.id,
                self.drag_from,
                self.drag_over,
            ));
        }

        let row = row(all)
            .spacing(2)
            .padding([0, 8])
            .align_y(iced::Alignment::End);

        container(mouse_area(row).on_release(Msg::DragDrop))
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
    id: TabId,
    drag_from: Option<usize>,
    drag_over: Option<usize>,
) -> Element<'a, Msg> {
    let is_dragged = drag_from == Some(i);
    let is_hover_target = drag_over == Some(i) && drag_from.is_some_and(|f| f != i);

    let label = text(name)
        .size(12)
        .color(if is_active { TEXT_PRIMARY } else { TEXT_MUTED });

    let close = button(
        text(crate::icons::X)
            .size(12)
            .font(crate::icons::LUCIDE)
            .color(TEXT_SECONDARY)
            .center(),
    )
    .on_press(Msg::Close(id))
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
        .on_press(Msg::Select(id))
        .style(move |_, status| {
            let is_hovered = matches!(status, button::Status::Hovered);

            let bg = if is_hover_target {
                Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.30)
            } else if is_dragged {
                Color::from_rgba(ACCENT.r, ACCENT.g, ACCENT.b, 0.10)
            } else if is_active {
                crate::theme::BG_ACTIVE
            } else if is_hovered {
                crate::theme::BG_HOVER
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

            let current_border_width = if is_hover_target {
                2.0
            } else if is_active {
                1.0
            } else {
                0.0
            };

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
