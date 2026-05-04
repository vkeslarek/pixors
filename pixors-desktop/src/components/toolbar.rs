use crate::theme::{
    ACCENT, ACCENT_DIM, BG_HOVER, BG_SURFACE, BORDER_SUBTLE, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY, TOOLBAR_W,
};
use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{Background, Border, Color, Element, Length};

#[derive(Debug, Clone)]
pub enum Msg {
    Select(Tool),
}

#[derive(Debug, Clone)]
pub struct State {
    pub active_tool: Tool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            active_tool: Tool::Move,
        }
    }
}

impl State {
    pub fn select(&mut self, tool: Tool) {
        self.active_tool = tool;
    }

    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Select(t) => self.active_tool = t,
        }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let groups = [
            Tool::GROUP1,
            Tool::GROUP2,
            Tool::GROUP3,
            Tool::GROUP4,
            Tool::GROUP5,
        ];

        let mut items: Vec<Element<Msg>> = Vec::new();
        for (gi, group) in groups.iter().enumerate() {
            if gi > 0 {
                items.push(
                    container(text(""))
                        .width(28)
                        .height(1)
                        .style(|_| container::Style {
                            background: Some(Background::Color(BORDER_SUBTLE)),
                            ..Default::default()
                        })
                        .padding([4, 0])
                        .into(),
                );
            }
            for t in group.iter().copied() {
                items.push(tool_btn(t, self.active_tool == t));
            }
        }


        let ctn = container(
            column(items)
                .spacing(2)
                .padding([4, 4])
                .align_x(iced::Alignment::Center),
        )
        .width(TOOLBAR_W)
        .height(Length::Shrink)
        .style(|_| container::Style {
            background: Some(Background::Color(BG_SURFACE)),
            border: Border {
                width: 0.0,
                color: BORDER_SUBTLE,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        container(row![ctn, iced::widget::space().height(Length::Fill)])
            .style(|_| container::Style {
                background: Some(Background::Color(BG_SURFACE)),
                border: Border {
                    width: 0.0,
                    color: BORDER_SUBTLE,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }).into()
    }
}

fn tool_btn(t: Tool, active: bool) -> Element<'static, Msg> {
    let icon_color = if active { ACCENT } else { TEXT_MUTED };
    let inner = container(
        text(t.icon())
            .size(18)
            .font(crate::icons::LUCIDE)
            .color(icon_color)
            .center(),
    )
    .width(40)
    .height(40)
    .center_x(Length::Fill)
    .center_y(Length::Fill);

    let btn = container(
        mouse_area(
            button(inner)
                .on_press(Msg::Select(t))
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    let bg = if active {
                        ACCENT_DIM
                    } else if hovered {
                        BG_HOVER
                    } else {
                        Color::TRANSPARENT
                    };
                    let border_color = if active {
                        crate::theme::ACCENT_GLOW
                    } else {
                        Color::TRANSPARENT
                    };
                    button::Style {
                        background: Some(Background::Color(bg)),
                        text_color: if active { ACCENT } else { TEXT_SECONDARY },
                        border: Border {
                            width: if active { 1.0 } else { 0.0 },
                            color: border_color,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                }),
        )
    )
    .width(Length::Shrink);

    crate::widgets::tooltip(btn, t.label(), iced::widget::tooltip::Position::Right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Move,
    Select,
    Lasso,
    Wand,
    Crop,
    Eyedropper,
    Brush,
    Eraser,
    Heal,
    Gradient,
    Text,
    Shape,
    Hand,
    Zoom,
}

impl Tool {
    pub fn label(&self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::Select => "Select",
            Tool::Lasso => "Lasso",
            Tool::Wand => "Wand",
            Tool::Crop => "Crop",
            Tool::Eyedropper => "Eyedropper",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Heal => "Heal",
            Tool::Gradient => "Gradient",
            Tool::Text => "Text",
            Tool::Shape => "Shape",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
        }
    }

    pub fn icon(&self) -> &'static str {
        use crate::icons;
        match self {
            Tool::Move => icons::MOVE,
            Tool::Select => icons::SQUARE,
            Tool::Lasso => icons::CIRCLE,
            Tool::Wand => icons::WAND,
            Tool::Crop => icons::CROP,
            Tool::Eyedropper => icons::DROPLET,
            Tool::Brush => icons::BRUSH,
            Tool::Eraser => icons::ERASER,
            Tool::Heal => icons::HEART,
            Tool::Gradient => icons::PALETTE,
            Tool::Text => icons::FILE_TEXT,
            Tool::Shape => icons::SHAPES,
            Tool::Hand => icons::HAND,
            Tool::Zoom => icons::ZOOM_IN,
        }
    }

    pub const GROUP1: &[Tool] = &[Tool::Move, Tool::Select, Tool::Lasso, Tool::Wand];
    pub const GROUP2: &[Tool] = &[Tool::Crop, Tool::Eyedropper];
    pub const GROUP3: &[Tool] = &[Tool::Brush, Tool::Eraser, Tool::Heal, Tool::Gradient];
    pub const GROUP4: &[Tool] = &[Tool::Text, Tool::Shape];
    pub const GROUP5: &[Tool] = &[Tool::Hand, Tool::Zoom];
}
