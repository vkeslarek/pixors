use tokio::sync::RwLock;
use std::collections::HashMap;
use uuid::Uuid;

/// Represents the physical state of the client's viewport.
#[derive(Debug, Clone)]
pub struct ViewportState {
    pub width: u32,
    pub height: u32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

/// Service responsible for tracking client viewports.
#[derive(Debug, Default)]
pub struct ViewportService {
    viewports: RwLock<HashMap<Uuid, ViewportState>>,
}

impl ViewportService {
    pub fn new() -> Self {
        Self {
            viewports: RwLock::new(HashMap::new()),
        }
    }

    /// Registers or updates a viewport state for a given session.
    pub async fn update_viewport(&self, session_id: &Uuid, state: ViewportState) {
        let mut viewports = self.viewports.write().await;
        viewports.insert(*session_id, state);
    }

    /// Retrieves the current viewport state for a given session.
    pub async fn get_viewport(&self, session_id: &Uuid) -> Option<ViewportState> {
        let viewports = self.viewports.read().await;
        viewports.get(session_id).cloned()
    }

    /// Removes a viewport state when a session is closed.
    pub async fn remove_viewport(&self, session_id: &Uuid) {
        let mut viewports = self.viewports.write().await;
        viewports.remove(session_id);
    }
}
