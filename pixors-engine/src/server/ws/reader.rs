use crate::server::app::AppState;
use crate::server::event_bus::EngineCommand;
use axum::extract::ws::Message;
use futures_util::{stream::SplitStream, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::types::{ClientFrame, ConnectionContext};

pub async fn run_reader_task(
    mut receiver: SplitStream<axum::extract::ws::WebSocket>,
    state: Arc<AppState>,
    frame_tx: mpsc::UnboundedSender<ClientFrame>,
    session_id: Uuid,
) {
    let mut ctx = ConnectionContext::new(frame_tx, session_id);

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Binary(data) => {
                if let Ok(cmd) = rmp_serde::from_slice::<EngineCommand>(&data) {
                    tracing::debug!("[WS] command received: {:?}", cmd);
                    state.route_command(cmd, &mut ctx).await;
                    if ctx.close_requested {
                        break;
                    }
                } else {
                    tracing::warn!("Invalid command received: {} bytes", data.len());
                }
            }
            Message::Close(_) => {
                tracing::info!("WebSocket closed by client");
                break;
            }
            _ => {}
        }
    }
}
