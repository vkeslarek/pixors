use crate::server::event_bus::EngineEvent;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SentTileKey {
    pub tab_id: Uuid,
    pub x: u32,
    pub y: u32,
}

/// A message sent from the reader task to the writer task.
/// This avoids broadcasting large binary data through the global event bus.
pub enum WriterMessage {
    /// A lightweight event from the global event bus (JSON only).
    Event(EngineEvent),
    /// Tile data: JSON metadata + binary pixel data, sent directly to this connection only.
    TileData {
        json: String,
        data: Vec<u8>,
    },
    /// Signal that all visible tiles have been sent.
    TilesComplete,
}
