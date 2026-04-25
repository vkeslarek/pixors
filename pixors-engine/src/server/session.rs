//! Session management for WebSocket connections.
//!
//! Tracks connected clients and their resource ownership (tabs).
//! Sessions are identified by a UUID passed as a WebSocket query parameter.
//! Disconnected sessions have a 30-second grace period before cleanup.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::server::app::AppState;
use crate::server::event_bus::EngineEvent;
use crate::server::service::session::SessionEvent;
use crate::server::service::tab::TabSessionData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SessionStatus {
    Connected,
    Disconnected,
}

#[derive(Debug)]
pub struct Session {
    pub id: Uuid,
    pub tab_session: TabSessionData,
    pub status: SessionStatus,
    pub last_status: Instant,
}

impl Session {
    fn new(id: Uuid) -> Self {
        Self {
            id,
            tab_session: TabSessionData::new(),
            status: SessionStatus::Connected,
            last_status: Instant::now(),
        }
    }

    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_status.elapsed() >= timeout
    }
}

#[derive(Debug)]
pub struct SessionManager {
    sessions: RwLock<HashMap<Uuid, Arc<RwLock<Session>>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a new session or resumes an existing one (reconnect).
    /// Returns (session_arc, was_resumed).
    pub async fn create_if_missing(&self, id: Uuid) -> (Arc<RwLock<Session>>, bool) {
        let mut sessions = self.sessions.write().await;
        if let Some(existing) = sessions.get(&id) {
            let mut s = existing.write().await;
            s.status = SessionStatus::Connected;
            s.last_status = Instant::now();
            (existing.clone(), true)
        } else {
            let arc = Arc::new(RwLock::new(Session::new(id)));
            sessions.insert(id, arc.clone());
            (arc, false)
        }
    }

    /// Marks a session as disconnected (starts expiry timer).
    pub async fn disconnect(&self, id: &Uuid) {
        if let Some(session) = self.sessions.read().await.get(id) {
            let mut s = session.write().await;
            s.status = SessionStatus::Disconnected;
            s.last_status = Instant::now();
        }
    }

    /// Updates the session's last activity timestamp (used for heartbeat).
    pub async fn update_activity(&self, id: &Uuid) {
        if let Some(session) = self.sessions.read().await.get(id) {
            let mut s = session.write().await;
            s.last_status = Instant::now();
        }
    }

    /// Retrieves a session by ID, if it exists.
    pub async fn get(&self, id: &Uuid) -> Option<Arc<RwLock<Session>>> {
        self.sessions.read().await.get(id).cloned()
    }

    /// Read-only access to a session's tab data.
    pub async fn with_tab_session<F, R>(&self, session_id: &Uuid, f: F) -> Option<R>
    where F: FnOnce(&TabSessionData) -> R
    {
        let s = self.get(session_id).await?;
        let session = s.read().await;
        Some(f(&session.tab_session))
    }

    /// Mutable access to a session's tab data.
    pub async fn with_tab_session_mut<F, R>(&self, session_id: &Uuid, f: F) -> Option<R>
    where F: FnOnce(&mut TabSessionData) -> R
    {
        let s = self.get(session_id).await?;
        let mut session = s.write().await;
        Some(f(&mut session.tab_session))
    }

    pub async fn remove_expired(&self, timeout: Duration) -> Vec<(Uuid, Vec<Uuid>)> {
        // Collect candidate IDs under read lock
        let expired_ids: Vec<Uuid> = {
            let sessions = self.sessions.read().await;
            let mut ids = Vec::new();
            for (id, session) in sessions.iter() {
                if session.read().await.is_expired(timeout) {
                    ids.push(*id);
                }
            }
            ids
        };

        // Remove under write lock, re-checking expiry to avoid races with reconnect
        let mut result = Vec::new();
        let mut sessions = self.sessions.write().await;
        for id in &expired_ids {
            if let Some(session) = sessions.get(id) {
                if !session.read().await.is_expired(timeout) {
                    continue;
                }
            }
            if let Some(session) = sessions.remove(id) {
                let s = session.read().await;
                result.push((*id, s.tab_session.tab_ids().copied().collect()));
            }
        }
        result
    }
}

/// Periodic task that removes expired sessions and cleans up their resources.
/// Runs every 5 seconds. Sessions disconnected >30s are removed.
pub async fn session_cleanup_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let expired = state
            .session_manager
            .remove_expired(Duration::from_secs(30))
            .await;
        for (_session_id, tabs) in &expired {
            for tab_id in tabs {
                state.tab_service.delete_tab_cleanup(tab_id);
                state.viewport_service.remove_viewport(tab_id).await;
            }
        }
        if !expired.is_empty() {
            tracing::info!("Cleaned up {} expired sessions", expired.len());
        }
    }
}

/// Periodic task that broadcasts heartbeat events to all connected clients.
/// Runs every 5 seconds. Clients respond with a Heartbeat command to keep
/// their session's `last_status` alive.
pub async fn heartbeat_broadcast_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        state
            .event_bus
            .broadcast(EngineEvent::Session(SessionEvent::Heartbeat))
            .await;
    }
}
