use crate::server::event_bus::EngineEvent;
use crate::server::protocol::PixelFormat;
use crate::server::state::AppState;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::types::{SentTileKey, WriterMessage};
use super::reader::run_reader_task;
use super::writer::run_writer_task;

/// Handle a single WebSocket connection.
pub async fn handle_connection(socket: WebSocket, state: Arc<AppState>, tab_id: Option<Uuid>) {
    tracing::info!("New WebSocket connection, tab_id: {:?}", tab_id);

    // Subscribe to the global event bus (for lightweight events only)
    let event_bus = state.event_bus();
    let event_rx = event_bus.subscribe().await;

    // Direct channel for tile data (reader → writer, no cloning to other subscribers)
    let (tile_tx, tile_rx) = mpsc::unbounded_channel::<WriterMessage>();

    // Split socket into sender and receiver
    let (mut sender, receiver) = socket.split();

    if let Some(tab_id) = tab_id {
        if let Some((width, height)) = state.image_info(&tab_id).await {
            let initial_event = EngineEvent::ImageLoaded {
                tab_id,
                width,
                height,
                format: PixelFormat::Rgba8,
            };

            if let Ok(json) = serde_json::to_string(&initial_event) {
                let _ = sender.send(Message::Text(json)).await;
            }
        }
    }

    // Spawn writer task: merges event bus events + direct tile data
    let writer_task = tokio::spawn(run_writer_task(sender, event_rx, tile_rx, tab_id));

    // Spawn reader task: receive messages from client and handle commands
    let reader_task = tokio::spawn(run_reader_task(receiver, state, tab_id, tile_tx));

    // Wait for either task to finish
    let mut writer_task = writer_task;
    let mut reader_task = reader_task;

    tokio::select! {
        _ = &mut writer_task => {
            reader_task.abort();
        },
        _ = &mut reader_task => {
            writer_task.abort();
        },
    }

    tracing::info!("Connection closed");
}
