use crate::error::Error;
use crate::image::TileGrid;
use crate::server::event_bus::EventBus;
use crate::server::service::tab::TabService;
use crate::server::service::file::FileService;
use crate::server::service::viewport::ViewportService;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Global application state shared across handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    event_bus: Arc<EventBus>,
    tab_service: Arc<TabService>,
    file_service: Arc<FileService>,
    viewport_service: Arc<ViewportService>,
}

impl AppState {
    /// Creates a new application state.
    pub fn new() -> Self {
        Self {
            event_bus: EventBus::new(),
            tab_service: Arc::new(TabService::new(256)), // default 256x256 tiles
            file_service: Arc::new(FileService::new()),
            viewport_service: Arc::new(ViewportService::new()),
        }
    }

    /// Creates a new tab and returns its ID.
    pub async fn create_tab(&self) -> Uuid {
        self.tab_service.create_tab().await
    }

    /// Opens an image in a tab.
    pub async fn open_image(&self, tab_id: &Uuid, path: impl AsRef<Path>) -> Result<(), Error> {
        self.tab_service.open_image(tab_id, path).await
    }

    /// Gets image info for a tab.
    pub async fn image_info(&self, tab_id: &Uuid) -> Option<(u32, u32)> {
        self.tab_service.image_info(tab_id).await
    }

    /// Gets the tile grid for a tab.
    pub async fn tile_grid(&self, tab_id: &Uuid) -> Option<TileGrid> {
        self.tab_service.tile_grid(tab_id).await
    }

    /// Retrieves tile pixel data as sRGB u8.
    pub async fn get_tile_rgba8(&self, tab_id: &Uuid, tile: &crate::image::Tile, mip_level: usize) -> Result<Vec<u8>, Error> {
        self.tab_service.get_tile_rgba8(tab_id, tile, mip_level).await
    }

    /// Returns true if a tab with the given ID exists.
    pub async fn tab_exists(&self, tab_id: &Uuid) -> bool {
        self.tab_service.list_tabs().await.contains(tab_id)
    }

    /// Deletes a tab.
    pub async fn delete_tab(&self, tab_id: &Uuid) -> bool {
        self.tab_service.delete_tab(tab_id).await
    }

    /// Lists all active tab IDs.
    pub async fn list_tabs(&self) -> Vec<Uuid> {
        self.tab_service.list_tabs().await
    }

    /// Returns a reference to the tab service.
    pub fn tab_service(&self) -> &Arc<TabService> {
        &self.tab_service
    }

    /// Returns a reference to the viewport service.
    pub fn viewport_service(&self) -> &Arc<ViewportService> {
        &self.viewport_service
    }

    /// Returns a reference to the event bus.
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }
}
