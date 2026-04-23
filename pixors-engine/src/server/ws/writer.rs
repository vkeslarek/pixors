use crate::server::event_bus::EngineEvent;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{stream::SplitSink, SinkExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::types::WriterMessage;

pub async fn run_writer_task(
    mut sender: SplitSink<WebSocket, Message>,
    mut event_rx: mpsc::UnboundedReceiver<EngineEvent>,
    mut tile_rx: mpsc::UnboundedReceiver<WriterMessage>,
    connection_tab_id: Option<Uuid>,
) {
    tracing::debug!("Writer task started for tab {:?}", connection_tab_id);
    loop {
        tokio::select! {
            // Handle lightweight events from the global event bus
            Some(event) = event_rx.recv() => {
                // Skip TileData events from the event bus — they shouldn't
                // be there, but guard against it.
                if matches!(&event, EngineEvent::TileData { .. }) {
                    continue;
                }

                if let Ok(json) = serde_json::to_string(&event) {
                    if sender.send(Message::Text(json)).await.is_err() {
                        tracing::warn!("Failed to send event to client");
                        break;
                    }
                }
            }
            // Handle direct tile data from the reader task
            Some(msg) = tile_rx.recv() => {
                match msg {
                    WriterMessage::Event(event) => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            if sender.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }
                    WriterMessage::TileData { json, data } => {
                        if sender.send(Message::Text(json)).await.is_err() {
                            tracing::warn!("Failed to send tile JSON to client");
                            break;
                        }
                        if sender.send(Message::Binary(data.into())).await.is_err() {
                            tracing::warn!("Failed to send tile binary to client");
                            break;
                        }
                    }
                    WriterMessage::TilesComplete => {
                        let json = serde_json::to_string(&EngineEvent::TilesComplete)
                            .unwrap_or_default();
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
            }
            else => break,
        }
    }
    tracing::debug!("Writer task ended for tab {:?}", connection_tab_id);
}
