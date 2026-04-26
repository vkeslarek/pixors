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
// Viewport — persistent tile cache in RAM. The server queries this for tiles.
// ═══════════════════════════════════════════════════════════════════════════

pub struct Viewport {
    tiles: RwLock<HashMap<(u32, TileCoord), Arc<Vec<u8>>>>,
    ready: AtomicBool,
    pub on_tile_added: Option<Arc<dyn Fn(u32, TileCoord, Arc<Vec<u8>>) + Send + Sync>>,
}

impl Viewport {
    pub fn new() -> Self {
        Self { tiles: RwLock::new(HashMap::new()), ready: AtomicBool::new(false), on_tile_added: None }
    }

    pub fn put(&self, mip: u32, coord: TileCoord, data: Arc<Vec<u8>>) {
        self.tiles.write().insert((mip, coord), data.clone());
        if let Some(cb) = &self.on_tile_added {
            cb(mip, coord, data);
        }
    }

    pub fn get(&self, mip: u32, coord: TileCoord) -> Option<Arc<Vec<u8>>> {
        self.tiles.read().get(&(mip, coord)).cloned()
    }

    pub fn mark_ready(&self) { self.ready.store(true, Ordering::Release); }
    pub fn is_ready(&self) -> bool { self.ready.load(Ordering::Acquire) }
}

// ═══════════════════════════════════════════════════════════════════════════
// ViewportSink — TileSink that feeds tiles into a Viewport.
// ═══════════════════════════════════════════════════════════════════════════

pub struct ViewportSink {
    viewport: Arc<Viewport>,
}

impl ViewportSink {
    pub fn new(viewport: Arc<Viewport>) -> Self { Self { viewport } }
}

impl TileSink for ViewportSink {
    fn run(&self, rx: mpsc::Receiver<Frame>) -> JoinHandle<()> {
        let vp = Arc::clone(&self.viewport);
        std::thread::spawn(move || {
            let mut tile_count = 0u32;
            while let Ok(frame) = rx.recv() {
                match frame.kind {
                    FrameKind::Tile { coord } => {
                        vp.put(frame.meta.mip_level, coord, Arc::new(frame.data.into_owned()));
                        tile_count += 1;
                    }
                    FrameKind::StreamDone => {
                        tracing::debug!("viewport_sink: stored {} tiles, marking ready", tile_count);
                        vp.mark_ready();
                        break;
                    }
                    _ => {}
                }
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WorkingSink — receives tiles, converts u8→f16, writes to disk.
// ═══════════════════════════════════════════════════════════════════════════

pub struct WorkingSink {
    store: Arc<WorkingWriter>,
}

impl WorkingSink {
    pub fn new(store: Arc<WorkingWriter>) -> Self { Self { store } }
}

impl TileSink for WorkingSink {
    fn run(&self, rx: mpsc::Receiver<Frame>) -> JoinHandle<()> {
        let store = Arc::clone(&self.store);
        std::thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                if let FrameKind::Tile { coord } = frame.kind {
                    let f16_pixels: Vec<crate::pixel::Rgba<half::f16>> =
                        bytemuck::cast_slice(&frame.data).to_vec();
                    if let Err(e) = store.write_tile_f16(&crate::image::Tile::new(coord, f16_pixels)) {
                        tracing::error!("WorkingSink: write failed: {}", e);
                    }
                }
                if frame.is_terminal() { break; }
            }
        })
    }
}
