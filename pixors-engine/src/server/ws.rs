use crate::server::protocol::{ClientCommand, PixelFormat, ServerEvent};
use crate::server::state::AppState;
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use std::sync::Arc;

/// Handle a single WebSocket connection.
pub async fn handle_connection(mut socket: WebSocket, state: Arc<AppState>) {
    tracing::info!("New WebSocket connection");

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                    let should_close = handle_command(cmd, &mut socket, &state).await;
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
) -> bool {
    match cmd {
        ClientCommand::LoadImage { path } => {
            handle_load_image(path, socket, state).await;
            false
        }
        ClientCommand::GetImageInfo => {
            handle_get_image_info(socket, state).await;
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

async fn handle_load_image(path: String, socket: &mut WebSocket, state: &Arc<AppState>) {
    tracing::info!("Loading image: {}", path);
    match state.load_image(&path).await {
        Ok((width, height)) => {
            send_json(socket, &ServerEvent::ImageLoaded {
                width,
                height,
                format: PixelFormat::Rgba8,
            }).await;

            // Send binary data
            if let Some(bytes) = state.to_rgba8().await {
                send_json(socket, &ServerEvent::BinaryData { size: bytes.len() }).await;
                let _ = socket.send(Message::Binary(bytes)).await;
            }
        }
        Err(e) => {
            send_error(socket, &e.to_string()).await;
        }
    }
}

async fn handle_get_image_info(socket: &mut WebSocket, state: &Arc<AppState>) {
    if let Some((width, height)) = state.image_info().await {
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
