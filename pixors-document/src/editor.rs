use pixors_engine::common::color::space::ColorSpace;
use pixors_engine::common::pixel::PixelFormat;

use super::tab::{Tab, TabId};

pub struct EditorState {
    pub tabs: Vec<Tab>,
    pub active: Option<TabId>,
    pub next_tab_id: u64,
    pub working_format: PixelFormat,
    pub working_color_space: ColorSpace,
    pub display_format: PixelFormat,
    pub display_color_space: ColorSpace,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_tab_id: 0,
            working_format: PixelFormat::RgbaF16,
            working_color_space: ColorSpace::ACES_CG,
            display_format: PixelFormat::Rgba8,
            display_color_space: ColorSpace::SRGB,
        }
    }

    pub fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub fn push_tab(&mut self, tab: Tab) {
        let id = tab.id;
        if tab.document.canvas.width <= 1 {
            // CanvasInfo defaults set via EditorState, no mutation needed
        }
        let title = tab
            .document
            .assets
            .primary_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        self.tabs.push(tab);
        self.active = Some(id);
        tracing::info!(
            "[state] push_tab id={id:?} title=\"{title}\" tab_count={} active={id:?}",
            self.tabs.len(),
        );
    }

    pub fn close(&mut self, id: TabId) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            let title = self.tabs[pos]
                .document
                .assets
                .primary_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("untitled")
                .to_string();
            self.tabs.remove(pos);
            let old_active = self.active;
            if self.active == Some(id) {
                self.active = self
                    .tabs
                    .get(pos)
                    .or_else(|| self.tabs.get(pos.saturating_sub(1)))
                    .map(|t| t.id);
            }
            tracing::info!(
                "[state] close_tab id={id:?} title=\"{title}\" active {:?}→{:?} tab_count={}",
                old_active,
                self.active,
                self.tabs.len(),
            );
        } else {
            tracing::warn!("[state] close_tab id={id:?} not found");
        }
    }

    pub fn switch(&mut self, id: TabId) {
        let old = self.active;
        self.active = Some(id);
        if old != self.active {
            let title = self
                .tab(id)
                .and_then(|t| t.document.assets.primary_path.as_ref())
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("untitled");
            tracing::info!("[state] switch_tab {:?}→{id:?} title=\"{title}\"", old);
        }
    }

    pub fn swap_tabs(&mut self, a: usize, b: usize) {
        if a < self.tabs.len() && b < self.tabs.len() {
            self.tabs.swap(a, b);
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active.and_then(|id| self.tab_mut(id))
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active
    }
}
