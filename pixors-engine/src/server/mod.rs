//! WebSocket server for Pixors engine.
//!
//! This module provides a WebSocket server that accepts commands from the frontend
//! and streams image data to connected clients.

mod app;
mod event_bus;
mod server;
mod service;
mod ws;

pub use event_bus::{EngineCommand, EngineEvent, EventBus};
pub use server::start_server;
