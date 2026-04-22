//! Session management for isolated image editing contexts.

use crate::error::Error;
use crate::image::TileGrid;
use crate::image::TypedImage;
use crate::pixel::Rgba;
use half::f16;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

/// State for a single editing session.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Unique identifier for the session.
    pub id: Uuid,
    /// When the session was created (Unix timestamp).
    pub created_at: u64,
    /// The working image loaded in this session, if any.
    pub typed_image: Option<Arc<TypedImage<Rgba<f16>>>>,
    /// Tile grid for the loaded image (if any).
    pub tile_grid: Option<TileGrid>,
    /// Tile size used for tiling (default: 256).
    pub tile_size: u32,
}

impl SessionState {
    /// Creates a new empty session.
    pub fn new(tile_size: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            typed_image: None,
            tile_grid: None,
            tile_size,
        }
    }

    /// Sets an already loaded/converted typed image into this session.
    pub fn set_image(&mut self, typed: TypedImage<Rgba<f16>>) -> Result<(u32, u32), Error> {
        let _sw = crate::debug_stopwatch!("session_set_image");
        tracing::info!("Setting image for session {}", self.id);
        
        let typed_arc = Arc::new(typed);
        
        // Create tile grid (zero-copy from working buffer)
        let tile_grid = TileGrid::new(Arc::clone(&typed_arc), self.tile_size);
        
        let width = typed_arc.width;
        let height = typed_arc.height;
        
        self.typed_image = Some(typed_arc);
        self.tile_grid = Some(tile_grid);
        
        Ok((width, height))
    }

    /// Returns image dimensions if an image is loaded.
    pub fn image_info(&self) -> Option<(u32, u32)> {
        self.typed_image.as_ref().map(|img| (img.width, img.height))
    }

    /// Returns the tile grid if an image is loaded.
    pub fn tile_grid(&self) -> Option<&TileGrid> {
        self.tile_grid.as_ref()
    }

    /// Returns the typed image if loaded.
    pub fn typed_image(&self) -> Option<&Arc<TypedImage<Rgba<f16>>>> {
        self.typed_image.as_ref()
    }

    /// Checks if the session has an image loaded.
    pub fn has_image(&self) -> bool {
        self.typed_image.is_some()
    }

    /// Clears the session (removes loaded image).
    pub fn clear(&mut self) {
        self.typed_image = None;
        self.tile_grid = None;
    }
}

/// Manages multiple concurrent sessions.
#[derive(Debug, Default)]
pub struct SessionService {
    sessions: RwLock<std::collections::HashMap<Uuid, SessionState>>,
    default_tile_size: u32,
}

impl SessionService {
    /// Creates a new session manager with the given default tile size.
    pub fn new(default_tile_size: u32) -> Self {
        Self {
            sessions: RwLock::new(std::collections::HashMap::new()),
            default_tile_size,
        }
    }

    /// Creates a new session and returns its ID.
    pub async fn create_session(&self) -> Uuid {
        let session = SessionState::new(self.default_tile_size);
        let id = session.id;
        
        let mut sessions = self.sessions.write().await;
        sessions.insert(id, session);
        
        id
    }

    /// Retrieves a session by ID.
    pub async fn get_session(&self, id: &Uuid) -> Option<SessionState> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    /// Retrieves a mutable reference to a session by ID.
    pub async fn get_session_mut(&self, _id: &Uuid) -> Option<tokio::sync::RwLockWriteGuard<'_, SessionState>> {
        // Note: This is a bit tricky because we need to get a write guard
        // to a specific entry in the HashMap. We'll implement a different approach.
        None
    }

    /// Sets an image into a session.
    pub async fn set_image(&self, session_id: &Uuid, typed: TypedImage<Rgba<f16>>) -> Result<(u32, u32), Error> {
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(session_id) {
            session.set_image(typed)
        } else {
            Err(Error::invalid_param(format!("Session {} not found", session_id)))
        }
    }

    /// Gets image info for a session.
    pub async fn image_info(&self, session_id: &Uuid) -> Option<(u32, u32)> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).and_then(|s| s.image_info())
    }

    /// Gets the tile grid for a session.
    pub async fn tile_grid(&self, session_id: &Uuid) -> Option<TileGrid> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).and_then(|s| s.tile_grid().cloned())
    }

    /// Deletes a session, freeing all its resources.
    pub async fn delete_session(&self, session_id: &Uuid) -> bool {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id).is_some()
    }

    /// Lists all active session IDs.
    pub async fn list_sessions(&self) -> Vec<Uuid> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// Cleans up old sessions (placeholder for future session expiration).
    pub async fn cleanup_old_sessions(&self, max_age_seconds: u64) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let mut sessions = self.sessions.write().await;
        let initial_len = sessions.len();
        
        sessions.retain(|_, session| {
            now.saturating_sub(session.created_at) < max_age_seconds
        });
        
        initial_len - sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_session_creation() {
        let manager = SessionService::new(256);
        let id = manager.create_session().await;
        
        assert!(manager.get_session(&id).await.is_some());
        assert_eq!(manager.list_sessions().await, vec![id]);
    }
    
    #[tokio::test]
    async fn test_session_deletion() {
        let manager = SessionService::new(256);
        let id = manager.create_session().await;
        
        assert!(manager.delete_session(&id).await);
        assert!(manager.get_session(&id).await.is_none());
        assert!(manager.list_sessions().await.is_empty());
    }
    
    #[tokio::test]
    async fn test_multiple_sessions() {
        let manager = SessionService::new(256);
        let id1 = manager.create_session().await;
        let id2 = manager.create_session().await;
        
        let sessions = manager.list_sessions().await;
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&id1));
        assert!(sessions.contains(&id2));
    }
    
    #[tokio::test]
    async fn test_cleanup_old_sessions() {
        let manager = SessionService::new(256);
        let id = manager.create_session().await;
        
        // Simulate an old session by modifying its creation time
        {
            let mut sessions = manager.sessions.write().await;
            if let Some(session) = sessions.get_mut(&id) {
                session.created_at = 0; // Very old
            }
        }
        
        let cleaned = manager.cleanup_old_sessions(3600).await;
        assert_eq!(cleaned, 1);
        assert!(manager.get_session(&id).await.is_none());
    }
}