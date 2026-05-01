use iced::widget::{button, column, container, text};
use iced::{Background, Border, Color, Element, Length};

use crate::ui::theme::{
    self, ACCENT, BG_BASE, BG_HOVER, BORDER_SUBTLE, TEXT_MUTED, TEXT_SECONDARY,
    WORKSPACE_BAR_W,
};

#[derive(Debug, Clone)]
pub enum Msg {
    Select(Workspace),
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub active: Workspace,
}

impl State {
    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Select(ws) => {
                if ws.available() {
                    self.active = ws;
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let items = [Workspace::Library, Workspace::Darkroom, Workspace::Editor];

        let buttons: Vec<Element<Msg>> = items
            .iter()
            .map(|ws| ws_btn(*ws, self.active == *ws))
            .collect();

        let footer_items: Vec<Element<Msg>> = vec![
            footer_btn(crate::ui::icons::SETTINGS),
            footer_btn(crate::ui::icons::HELP),
        ];

        let layout = column![
            column(buttons).spacing(2).padding(4),
            iced::widget::vertical_space(),
            column(footer_items).spacing(2).padding(4),
        ]
        .height(Length::Fill);

        container(layout)
            .width(WORKSPACE_BAR_W)
            .height(Length::Fill)
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

fn ws_btn(ws: Workspace, is_active: bool) -> Element<'static, Msg> {
    let available = ws.available();
    let icon_color = if is_active {
        ACCENT
    } else if available {
        TEXT_SECONDARY
    } else {
        Color::from_rgba(theme::TEXT_MUTED.r, theme::TEXT_MUTED.g, theme::TEXT_MUTED.b, 0.4)
    };
    let icon = text(ws.icon())
        .size(20)
        .font(crate::ui::icons::LUCIDE)
        .color(icon_color)
        .center();

    let inner = container(icon)
        .width(Length::Fill)
        .height(40)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default().rounded(8),
            ..Default::default()
        });

    let mut btn = button(inner)
        .width(Length::Fill)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            let bg = if hovered && available {
                BG_HOVER
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border::default().rounded(8),
                text_color: icon_color,
                ..Default::default()
            }
        });
    if available {
        btn = btn.on_press(Msg::Select(ws));
    }
    btn.into()
}

fn footer_btn(label: &'static str) -> Element<'static, Msg> {
    button(
        container(
            text(label)
                .size(16)
                .font(crate::ui::icons::LUCIDE)
                .color(TEXT_MUTED)
                .center(),
        )
            .width(Length::Fill)
            .height(34)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fill)
    .style(|_, status| {
        let hovered = matches!(status, button::Status::Hovered);
        let bg = if hovered { BG_HOVER } else { Color::TRANSPARENT };
        button::Style {
            background: Some(Background::Color(bg)),
            border: Border::default().rounded(8),
            text_color: TEXT_MUTED,
            ..Default::default()
        }
    })
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Workspace {
    Library,
    Darkroom,
    #[default]
    Editor,
}

impl Workspace {
    pub fn label(&self) -> &'static str {
        match self {
            Workspace::Library => "Library",
            Workspace::Darkroom => "Darkroom",
            Workspace::Editor => "Editor",
        }
    }

    pub fn icon(&self) -> &'static str {
        use crate::ui::icons;
        match self {
            Workspace::Library => icons::IMAGES,
            Workspace::Darkroom => icons::SUN,
            Workspace::Editor => icons::LAYERS,
        }
    }

    pub fn available(&self) -> bool {
        true
    }
}
