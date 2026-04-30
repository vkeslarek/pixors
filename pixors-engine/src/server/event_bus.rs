//! Event bus for unidirectional communication between engine and clients.
//!
//! The engine is the single source of truth. Clients send commands, engine broadcasts events.
//! All state mutations happen through commands; clients update their local state only after
//! receiving corresponding events.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::server::service::filter::{FilterCommand, FilterEvent};
use crate::server::service::layer::{LayerCommand, LayerEvent};
use crate::server::service::loader::{LoaderCommand, LoaderEvent};
use crate::server::service::session::{SessionCommand, SessionEvent};
use crate::server::service::tab::{TabCommand, TabEvent};
use crate::server::service::tool::{ToolCommand, ToolEvent};
use crate::server::service::viewport::{ViewportCommand, ViewportEvent};

/// Events sent from engine to all connected clients.
///
/// Each variant wraps an event type defined alongside the service
/// or system component that produces it. `#[serde(untagged)]` makes
/// serde serialize the inner value directly, preserving the type tag
/// from each wrapped enum.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EngineEvent {
    Session(SessionEvent),
    Tab(TabEvent),
    Layer(LayerEvent),
    Loader(LoaderEvent),
    Viewport(ViewportEvent),
    Tool(ToolEvent),
    Filter(FilterEvent),
    Error { message: String },
}

/// Commands sent from client to engine.
///
/// Each variant wraps a command type defined alongside the service
/// that handles it. `#[serde(untagged)]` makes serde deserialize
/// the inner value directly, using each wrapped enum's own type tag.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EngineCommand {
    Session(SessionCommand),
    Tab(TabCommand),
    Layer(LayerCommand),
    Loader(LoaderCommand),
    Viewport(ViewportCommand),
    Tool(ToolCommand),
    Filter(FilterCommand),
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
}
