use crate::app::App;
use crate::page::editor::tab_bar;

impl App {
    pub(crate) fn handle_tab_bar(&mut self, m: tab_bar::Msg) {
        match m {
            tab_bar::Msg::Select(id) => {
                self.state.switch(id);
                self.update_status_from_active_tab();
            }
            tab_bar::Msg::Close(id) => {
                self.viewport_tabs.remove(&id);
                self.state.close(id);
                self.dispatcher.cleanup_tab(id);
                self.update_status_from_active_tab();
            }
            tab_bar::Msg::DragDrop => {
                if let (Some(from), Some(to)) = (self.tabs.drag_from, self.tabs.drag_over)
                    && from != to
                {
                    self.state.swap(from, to);
                }
                self.tabs.drag_from = None;
                self.tabs.drag_over = None;
            }
            _ => self.tabs.update(m, self.state.all_sessions().len()),
        }
    }
}
