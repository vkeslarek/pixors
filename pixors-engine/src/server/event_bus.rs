//! Event bus for unidirectional communication between engine and clients.
//!
//! The engine is the single source of truth. Clients send commands, engine broadcasts events.
//! All state mutations happen through commands; clients update their local state only after
//! receiving corresponding events.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use super::protocol::PixelFormat;

// -----------------------------------------------------------------------------
// EngineEvent (broadcast from engine to all clients)
// -----------------------------------------------------------------------------

/// Events sent from engine to all connected clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    // Tab lifecycle
    TabCreated {
        tab_id: Uuid,
        name: String,
    },
    TabClosed {
        tab_id: Uuid,
    },
    TabActivated {
        tab_id: Uuid,
    },

    // Image lifecycle
    ImageLoaded {
        tab_id: Uuid,
        width: u32,
        height: u32,
        format: PixelFormat,
    },
    ImageClosed {
        tab_id: Uuid,
    },

    // Tile streaming (per-tab WS only)
    TileData {
        tab_id: Uuid,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        mip_level: usize,
        size: usize,
        #[serde(skip_serializing)]
        data: Vec<u8>,
    },
    TilesComplete,
    TilesDirty {
        tab_id: Uuid,
        regions: Vec<TileRect>,
    },

    // Tool / UI state
    ToolChanged {
        tool: String,
    },
    ViewportUpdated {
        tab_id: Uuid,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
    },

    // Errors
    Error {
        message: String,
    },
}

// -----------------------------------------------------------------------------
// EngineCommand (sent from client to engine)
// -----------------------------------------------------------------------------

/// Commands sent from client to engine.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineCommand {
    CreateTab,
    CloseTab {
        tab_id: Uuid,
    },
    ActivateTab {
        tab_id: Uuid,
    },
    OpenFile {
        tab_id: Uuid,
        path: String,
    },
    ViewportUpdate {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        zoom: f32,
    },
    SelectTool {
        tool: String,
    },
    GetState,
    Screenshot,
    Close,
    /// Mark tiles as dirty (for testing tile invalidation).
    MarkTilesDirty {
        tab_id: Uuid,
        regions: Vec<TileRect>,
    },
}

// -----------------------------------------------------------------------------
// Supporting types
// -----------------------------------------------------------------------------



/// Rectangle describing a tile region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// -----------------------------------------------------------------------------
// EventBus
// -----------------------------------------------------------------------------

/// Global event bus for broadcasting engine events to all connected clients.
#[derive(Debug)]
pub struct EventBus {
    subscribers: RwLock<Vec<mpsc::UnboundedSender<EngineEvent>>>,
}

impl EventBus {
    /// Creates a new event bus.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            subscribers: RwLock::new(Vec::new()),
        })
    }

    /// Subscribes to the event bus, returning a receiver for events.
    pub async fn subscribe(&self) -> mpsc::UnboundedReceiver<EngineEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.subscribers.write().await;
        subscribers.push(tx);
        rx
    }

    /// Broadcasts an event to all subscribers.
    pub async fn broadcast(&self, event: EngineEvent) {
        let mut subscribers = self.subscribers.write().await;
        subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Broadcasts an event only to subscribers interested in a specific tab.
    /// For now, we broadcast to all; per-tab filtering can be added later.
    pub async fn broadcast_to_tab(&self, _tab_id: &Uuid, event: EngineEvent) {
        self.broadcast(event).await;
    }
}
