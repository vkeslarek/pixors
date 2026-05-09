use crate::theme::{BG_SURFACE, BORDER_SUBTLE, TOOLBAR_W};
use crate::layout::sidebar;
use iced::widget::{column, container, row, text};
use iced::{Background, Element, Length};

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
        .width(Length::Fill)
        .height(Length::Shrink);

        sidebar(row![ctn, iced::widget::space().height(Length::Fill)])
            .width(TOOLBAR_W)
            .background(BG_SURFACE)
            .into()
    }
}

fn tool_btn(t: Tool, active: bool) -> Element<'static, Msg> {
    let btn = crate::components::icon_button(t.icon())
        .size(18)
        .width(40)
        .height(40)
        .active(active)
        .on_press(Msg::Select(t));

    crate::components::tooltip::tooltip(
        iced::widget::container(iced::widget::mouse_area(btn)).width(Length::Shrink),
        t.label(),
        iced::widget::tooltip::Position::Right
    )
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
