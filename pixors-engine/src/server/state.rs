use crate::error::Error;
use crate::image::TileGrid;
use crate::server::service::session::SessionService;
use crate::server::service::file::FileService;
use crate::server::service::viewport::ViewportService;
use std::sync::Arc;
use uuid::Uuid;

/// Global application state shared across handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    session_service: Arc<SessionService>,
    file_service: Arc<FileService>,
    viewport_service: Arc<ViewportService>,
}

impl AppState {
    /// Creates a new application state.
    pub fn new() -> Self {
        Self {
            session_service: Arc::new(SessionService::new(256)), // default 256x256 tiles
            file_service: Arc::new(FileService::new()),
            viewport_service: Arc::new(ViewportService::new()),
        }
    }

    /// Creates a new session and returns its ID.
    pub async fn create_session(&self) -> Uuid {
        self.session_service.create_session().await
    }

    /// Loads an image into a session via FileService.
    pub async fn load_image(&self, session_id: &Uuid, path: &str) -> Result<(u32, u32), Error> {
        let typed = self.file_service.open_image(path).await?;
        self.session_service.set_image(session_id, typed).await
    }

    /// Gets image info for a session.
    pub async fn image_info(&self, session_id: &Uuid) -> Option<(u32, u32)> {
        self.session_service.image_info(session_id).await
    }

    /// Gets the tile grid for a session.
    pub async fn tile_grid(&self, session_id: &Uuid) -> Option<TileGrid> {
        self.session_service.tile_grid(session_id).await
    }

    /// Returns true if a session with the given ID exists (even if no image is loaded).
    pub async fn session_exists(&self, session_id: &Uuid) -> bool {
        self.session_service.get_session(session_id).await.is_some()
    }

    /// Deletes a session.
    pub async fn delete_session(&self, session_id: &Uuid) -> bool {
        self.session_service.delete_session(session_id).await
    }

    /// Returns a reference to the session service.
    pub fn session_service(&self) -> &Arc<SessionService> {
        &self.session_service
    }

    /// Returns a reference to the viewport service.
    pub fn viewport_service(&self) -> &Arc<ViewportService> {
        &self.viewport_service
    }
}
