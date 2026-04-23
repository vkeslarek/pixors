//! WebSocket server for Pixors engine.
//!
//! This module provides a WebSocket server that accepts commands from the frontend
//! and streams image data to connected clients.

mod event_bus;
mod protocol;
mod router;
mod service;
mod state;
mod ws;

pub use event_bus::{EngineCommand, EngineEvent, EventBus, TileRect};
pub use protocol::{ClientCommand, ServerEvent, PixelFormat};
pub use router::start_server;