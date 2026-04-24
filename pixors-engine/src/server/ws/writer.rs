use crate::server::event_bus::EngineEvent;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{stream::SplitSink, SinkExt};
use tokio::sync::mpsc;

use super::types::{ClientFrame, MSG_EVENT};

pub async fn run_writer_task(
    mut sender: SplitSink<WebSocket, Message>,
    mut event_rx: mpsc::UnboundedReceiver<EngineEvent>,
    mut frame_rx: mpsc::UnboundedReceiver<ClientFrame>,
) {
    tracing::debug!("Writer task started");
    loop {
        tokio::select! {
            biased;
            Some(event) = event_rx.recv() => {
                if let Some(bytes) = encode_event(event) {
                    if sender.send(Message::Binary(bytes)).await.is_err() {
                        tracing::warn!("Failed to send event to client");
                        break;
                    }
                }
            }
            Some(frame) = frame_rx.recv() => {
                let bytes = build_tlv(frame);
                if sender.send(Message::Binary(bytes)).await.is_err() {
                    tracing::warn!("Failed to send frame to client");
                    break;
                }
                drain_pending_events(&mut sender, &mut event_rx).await;
            }
            else => break,
        }
    }
    tracing::debug!("Writer task ended");
}

/// Encode an EngineEvent as a MSG_EVENT TLV frame.
fn encode_event(event: EngineEvent) -> Option<Vec<u8>> {
    let payload = rmp_serde::to_vec_named(&event).ok()?;
    Some(build_tlv(ClientFrame::new(MSG_EVENT, payload)))
}

/// Wrap a ClientFrame in [type_tag][4B payload_len_LE][payload].
fn build_tlv(frame: ClientFrame) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + frame.payload.len());
    buf.push(frame.type_tag);
    buf.extend_from_slice(&(frame.payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&frame.payload);
    buf
}

async fn drain_pending_events(
    sender: &mut SplitSink<WebSocket, Message>,
    event_rx: &mut mpsc::UnboundedReceiver<EngineEvent>,
) {
    loop {
        match event_rx.try_recv() {
            Ok(event) => {
                if let Some(bytes) = encode_event(event) {
                    if sender.send(Message::Binary(bytes)).await.is_err() {
                        return;
                    }
                }
            }
            Err(_) => return,
        }
    }
}
