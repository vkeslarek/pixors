use crate::server::protocol::{ClientCommand, PixelFormat, ServerEvent};
use crate::server::state::AppState;
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use std::sync::Arc;
use uuid::Uuid;

/// Handle a single WebSocket connection.
pub async fn handle_connection(mut socket: WebSocket, state: Arc<AppState>, session_id: Option<Uuid>) {
    tracing::info!("New WebSocket connection, session_id: {:?}", session_id);

    // Validate session if provided — check existence, not image presence
    if let Some(sid) = &session_id {
        if state.session_exists(sid).await {
            // If the session already has an image, stream tiles immediately
            if state.image_info(sid).await.is_some() {
                if let Err(e) = stream_tiles(&mut socket, &state, sid).await {
                    tracing::warn!("Failed to stream initial tiles: {}", e);
                }
            }
        } else {
            send_error(&mut socket, "Invalid or expired session").await;
            return;
        }
    }

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                    let should_close = handle_command(cmd, &mut socket, &state, session_id).await;
                    if should_close {
                        break;
                    }
                } else {
                    send_error(&mut socket, "Invalid command").await;
                }
            }
            Message::Binary(_) => {
                send_error(&mut socket, "Unexpected binary message").await;
            }
            Message::Close(_) => {
                tracing::info!("WebSocket closed by client");
                break;
            }
            _ => {}
        }
    }
    tracing::info!("Connection closed");
}

/// Dispatches a command to the appropriate handler.
/// Returns `true` if the connection should be closed.
async fn handle_command(
    cmd: ClientCommand,
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    session_id: Option<Uuid>,
) -> bool {
    match cmd {
        ClientCommand::LoadImage { path } => {
            handle_load_image(path, socket, state, session_id).await;
            false
        }
        ClientCommand::GetImageInfo => {
            handle_get_image_info(socket, state, session_id).await;
            false
        }
        ClientCommand::ApplyOperation { op, params: _ } => {
            handle_apply_operation(op, socket).await;
            false
        }
        ClientCommand::Close => {
            tracing::info!("Client requested close");
            true
        }
    }
}

async fn handle_load_image(
    path: String,
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    session_id: Option<Uuid>,
) {
    tracing::info!("Loading image: {}", path);
    
    let Some(session_id) = session_id else {
        send_error(socket, "No session ID provided").await;
        return;
    };
    
    match state.load_image(&session_id, &path).await {
        Ok((width, height)) => {
            send_json(socket, &ServerEvent::ImageLoaded {
                width,
                height,
                format: PixelFormat::Rgba8,
            }).await;

            // Stream tiles progressively
            if let Err(e) = stream_tiles(socket, state, &session_id).await {
                send_error(socket, &format!("Failed to stream tiles: {}", e)).await;
            }
        }
        Err(e) => {
            send_error(socket, &e.to_string()).await;
        }
    }
}

/// Stream all tiles of the image in the given session.
async fn stream_tiles(
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    session_id: &Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _sw = crate::debug_stopwatch!("ws_stream_tiles");
    let Some(tile_grid) = state.tile_grid(session_id).await else {
        return Err("No image loaded in session".into());
    };
    
    for tile in tile_grid.tiles() {
        let rgba8 = tile_grid.extract_tile_rgba8(tile)?;
        let size = rgba8.len();
        
        // Send tile metadata
        send_json(socket, &ServerEvent::TileData {
            x: tile.x,
            y: tile.y,
            width: tile.width,
            height: tile.height,
            size,
        }).await;
        
        // Send binary pixel data
        if let Err(e) = socket.send(Message::Binary(rgba8)).await {
            tracing::warn!("Failed to send tile binary data: {}", e);
            break;
        }
        
        // Small yield to avoid blocking the event loop
        tokio::task::yield_now().await;
    }
    
    Ok(())
}

async fn handle_get_image_info(
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    session_id: Option<Uuid>,
) {
    let Some(session_id) = session_id else {
        send_error(socket, "No session ID provided").await;
        return;
    };
    
    if let Some((width, height)) = state.image_info(&session_id).await {
        send_json(socket, &ServerEvent::ImageInfo {
            width,
            height,
            format: PixelFormat::Rgba8,
        }).await;
    } else {
        send_error(socket, "No image loaded").await;
    }
}

async fn handle_apply_operation(op: String, socket: &mut WebSocket) {
    tracing::info!("Operation {} not implemented yet", op);
    send_error(socket, &format!("Operation {} not implemented", op)).await;
}

/// Helper to send a JSON serialized event
async fn send_json(socket: &mut WebSocket, event: &ServerEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        let _ = socket.send(Message::Text(json)).await;
    }
}

/// Helper to send an error event
async fn send_error(socket: &mut WebSocket, message: &str) {
    let event = ServerEvent::Error {
        message: message.to_string(),
    };
    send_json(socket, &event).await;
}
