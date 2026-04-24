use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Wire protocol type tags (server → client TLV framing)
// ---------------------------------------------------------------------------
//
// Adding a new binary message type:
//  1. Reserve a constant here.
//  2. Send `ClientFrame { type_tag: MSG_*, payload: ... }` from your service.
//     No other files need to change.

/// msgpack-encoded EngineEvent
pub const MSG_EVENT: u8 = 0x00;
/// Tile pixel data: 36-byte header + RGBA8 pixels (see viewport service)
pub const MSG_TILE: u8 = 0x01;
/// All visible tiles have been sent for the current viewport
pub const MSG_TILES_COMPLETE: u8 = 0x02;

// ---------------------------------------------------------------------------
// ClientFrame — the only thing the writer task sends
// ---------------------------------------------------------------------------

/// A binary frame delivered directly to one client, bypassing the event bus.
/// The writer wraps it in `[type_tag][4B payload_len_LE][payload]` and sends it.
pub struct ClientFrame {
    pub type_tag: u8,
    /// Empty vec → 0-byte payload (e.g. TilesComplete).
    pub payload: Vec<u8>,
}

impl ClientFrame {
    pub fn new(type_tag: u8, payload: Vec<u8>) -> Self {
        Self { type_tag, payload }
    }

    pub fn empty(type_tag: u8) -> Self {
        Self { type_tag, payload: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// ConnectionContext — per-connection state passed to service handlers
// ---------------------------------------------------------------------------

/// Per-connection state passed to service command handlers.
pub struct ConnectionContext {
    /// Channel to push binary frames directly to this connection's writer task.
    pub frame_tx: mpsc::UnboundedSender<ClientFrame>,
    pub close_requested: bool,
}

impl ConnectionContext {
    pub fn new(frame_tx: mpsc::UnboundedSender<ClientFrame>) -> Self {
        Self {
            frame_tx,
            close_requested: false,
        }
    }
}
