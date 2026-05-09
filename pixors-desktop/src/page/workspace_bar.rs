use iced::widget::{button, column, container, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::{
    self, ACCENT, BG_BASE, BG_HOVER, TEXT_MUTED, TEXT_SECONDARY, WORKSPACE_BAR_W,
};
use crate::layout::sidebar;

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
            crate::components::icon_button(crate::icons::SETTINGS).size(16).into(),
            crate::components::icon_button(crate::icons::HELP).size(16).into(),
        ];

        let layout = column![
            column(buttons).spacing(2).padding(4),
            iced::widget::Space::new().height(Length::Fill),
            column(footer_items).spacing(2).padding(4),
        ]
        .height(Length::Fill);

        sidebar(layout)
            .width(WORKSPACE_BAR_W)
            .background(BG_BASE)
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
        Color::from_rgba(
            theme::TEXT_MUTED.r,
            theme::TEXT_MUTED.g,
            theme::TEXT_MUTED.b,
            0.4,
        )
    };
    let icon = text(ws.icon())
        .size(20)
        .font(crate::icons::LUCIDE)
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

    let mut btn = button(inner).width(Length::Fill).style(move |_, status| {
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


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Workspace {
    Library,
    Darkroom,
    #[default]
    Editor,
}

impl Workspace {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Workspace::Library => "Library",
            Workspace::Darkroom => "Darkroom",
            Workspace::Editor => "Editor",
        }
    }

    pub fn icon(&self) -> &'static str {
        use crate::icons;
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
