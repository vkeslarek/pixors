use crate::server::app::AppState;
use axum::extract::ws::WebSocket;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::types::ClientFrame;
use super::reader::run_reader_task;
use super::writer::run_writer_task;

/// Handle a single WebSocket connection.
pub async fn handle_connection(socket: WebSocket, state: Arc<AppState>) {
    tracing::info!("New WebSocket connection");

    let event_rx = state.event_bus.subscribe().await;
    let (frame_tx, frame_rx) = mpsc::unbounded_channel::<ClientFrame>();

    let (sender, receiver) = socket.split();

    let writer_task = tokio::spawn(run_writer_task(sender, event_rx, frame_rx));
    let reader_task = tokio::spawn(run_reader_task(receiver, state, frame_tx));

    let mut writer_task = writer_task;
    let mut reader_task = reader_task;

    tokio::select! {
        _ = &mut writer_task => { reader_task.abort(); }
        _ = &mut reader_task => { writer_task.abort(); }
    }

    tracing::info!("Connection closed");
}
