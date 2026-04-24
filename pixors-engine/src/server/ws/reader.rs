use crate::server::app::AppState;
use crate::server::event_bus::EngineCommand;
use axum::extract::ws::Message;
use futures_util::{stream::SplitStream, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::types::{ClientFrame, ConnectionContext};

pub async fn run_reader_task(
    mut receiver: SplitStream<axum::extract::ws::WebSocket>,
    state: Arc<AppState>,
    frame_tx: mpsc::UnboundedSender<ClientFrame>,
) {
    let mut ctx = ConnectionContext::new(frame_tx);

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Binary(data) => {
                if let Ok(cmd) = rmp_serde::from_slice::<EngineCommand>(&data) {
                    state.route_command(cmd, &mut ctx).await;
                    if ctx.close_requested {
                        break;
                    }
                } else if let Err(e) = rmp_serde::from_slice::<EngineCommand>(&data) {
                    tracing::warn!("Invalid command received: {} bytes, error: {}", data.len(), e);
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
