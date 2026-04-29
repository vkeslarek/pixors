use crate::image::TileCoord;
use crate::storage::writer::WorkingWriter;
use crate::stream::{Frame, FrameKind};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

/// A consumer that drains a Frame stream in its own thread.
pub trait TileSink: Send + Sync + 'static {
    fn run(&self, rx: mpsc::Receiver<Frame>) -> JoinHandle<()>;
}

// ═══════════════════════════════════════════════════════════════════════════
// WorkingSink — receives tiles, converts u8→f16, writes to disk.
// ═══════════════════════════════════════════════════════════════════════════


