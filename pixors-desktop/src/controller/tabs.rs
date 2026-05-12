use std::sync::Arc;

use crate::app::App;
use crate::page::editor::tab_bar;

impl App {
    pub(crate) fn handle_tab_bar(&mut self, m: tab_bar::Msg) {
        match m {
            tab_bar::Msg::Select(id) => {
                self.dispatcher.mutate(&mut self.state, |s| s.switch(id));
                self.update_status_from_active_tab();
            }
            tab_bar::Msg::Close(id) => {
                self.viewport_tabs.remove(&id);
                crate::viewport::tile_cache_sink::unregister_tile_cache(id.0);

                if let Err(e) = self.dispatcher.dispatch(
                    Arc::new(pixors_document::action::actions::close_tab::CloseTab(id)),
                    &mut self.state,
                ) {
                    self.push_error(e);
                }
                self.dispatcher.cleanup_tab(id);
                self.update_status_from_active_tab();
            }
            tab_bar::Msg::DragDrop => {
                if let (Some(from), Some(to)) = (self.tabs.drag_from, self.tabs.drag_over)
                    && from != to
                {
                    self.state.swap_tabs(from, to);
                }
                self.tabs.drag_from = None;
                self.tabs.drag_over = None;
            }
            _ => self.tabs.update(m, self.state.tabs().len()),
        }
    }
}
