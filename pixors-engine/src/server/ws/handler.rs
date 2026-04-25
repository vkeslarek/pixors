use crate::server::app::AppState;
use axum::extract::ws::WebSocket;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::types::ClientFrame;
use super::reader::run_reader_task;
use super::writer::run_writer_task;

/// Handle a single WebSocket connection.
pub async fn handle_connection(socket: WebSocket, state: Arc<AppState>, session_id: Option<Uuid>) {
    let sid = session_id.unwrap_or_else(Uuid::new_v4);
    let (_session_arc, was_resumed) = state.session_manager.create_if_missing(sid).await;
    if was_resumed {
        tracing::info!("Session {} resumed (reconnect)", sid);
    } else {
        tracing::info!("Session {} created", sid);
    }

    let event_rx = state.event_bus.subscribe().await;
    let (frame_tx, frame_rx) = mpsc::unbounded_channel::<ClientFrame>();

    let (sender, receiver) = socket.split();

    let writer_task = tokio::spawn(run_writer_task(sender, event_rx, frame_rx));
    let reader_task = tokio::spawn(run_reader_task(receiver, state.clone(), frame_tx, sid));

    let mut writer_task = writer_task;
    let mut reader_task = reader_task;

    tokio::select! {
        _ = &mut writer_task => { reader_task.abort(); }
        _ = &mut reader_task => { writer_task.abort(); }
    }

    state.session_manager.disconnect(&sid).await;
    tracing::info!("Session {} disconnected", sid);
}
