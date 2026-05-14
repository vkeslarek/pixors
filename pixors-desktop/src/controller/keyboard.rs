use iced::keyboard::{self, Key};
use iced::Task;

use crate::app::{App, Msg};
use crate::page::editor::toolbar::Tool;

impl App {
    pub(crate) fn handle_keyboard(&mut self, event: keyboard::Event) -> Task<Msg> {
        if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
            if modifiers.contains(keyboard::Modifiers::CTRL) {
                match key.as_ref() {
                    Key::Character("o") => return self.open_file_dialog(),
                    Key::Character("e") if self.active_file_path().is_some() => {
                        self.show_export_dialog = true;
                    }
                    _ => {}
                }
            } else {
                match key.as_ref() {
                    Key::Character("v") => self.tools.select(Tool::Move),
                    Key::Character("m") => self.tools.select(Tool::Select),
                    Key::Character("l") => self.tools.select(Tool::Lasso),
                    Key::Character("w") => self.tools.select(Tool::Wand),
                    Key::Character("c") => self.tools.select(Tool::Crop),
                    Key::Character("i") => self.tools.select(Tool::Eyedropper),
                    Key::Character("b") => self.tools.select(Tool::Brush),
                    Key::Character("e") => self.tools.select(Tool::Eraser),
                    Key::Character("j") => self.tools.select(Tool::Heal),
                    Key::Character("g") => self.tools.select(Tool::Gradient),
                    Key::Character("t") => self.tools.select(Tool::Text),
                    Key::Character("u") => self.tools.select(Tool::Shape),
                    Key::Character("h") => self.tools.select(Tool::Hand),
                    Key::Character("z") => self.tools.select(Tool::Zoom),
                    _ => {}
                }
            }
            self.status.active_tool = self.tools.active_tool;
        }
        Task::none()
    }
}
