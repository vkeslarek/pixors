use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::{Device, Stage, StageRole};
use crate::container::Tile;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::SinkRunner;
use crate::error::Error;
use crate::debug_stopwatch;

// ── Shared GPU buffer ───────────────────────────────────────────────────────

#[derive(Debug)]
pub struct GpuBufferState {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
}

static DISPLAY_BUFFER: OnceLock<Arc<Mutex<GpuBufferState>>> = OnceLock::new();

pub fn init_buffer(w: u32, h: u32) -> Arc<Mutex<GpuBufferState>> {
    let buf = Arc::new(Mutex::new(GpuBufferState {
        pixels: vec![0u8; (w * h * 4) as usize],
        width: w,
        height: h,
        dirty: false,
    }));
    let _ = DISPLAY_BUFFER.set(buf.clone());
    buf
}

pub fn display_buffer() -> Option<Arc<Mutex<GpuBufferState>>> {
    DISPLAY_BUFFER.get().cloned()
}

// ── Stage ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySink;

impl Stage for DisplaySink {
    fn kind(&self) -> &'static str {
        "display_sink"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
    fn role(&self) -> StageRole {
        StageRole::Sink
    }
    fn sink_runner(&self) -> Result<Box<dyn SinkRunner>, Error> {
        let buf = DISPLAY_BUFFER
            .get()
            .cloned()
            .ok_or_else(|| Error::internal("display buffer not initialized"))?;
        Ok(Box::new(DisplaySinkRunner { buf }))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

pub struct DisplaySinkRunner {
    buf: Arc<Mutex<GpuBufferState>>,
}

fn copy_tile_to_rgba8(dst: &mut [u8], dst_width: u32, src: &[u8], tile: &Tile) {
    let bpp = 4usize;
    let stride = dst_width as usize * bpp;
    let tw = tile.coord.width as usize;
    let tpx = tile.coord.px as usize;
    let tpy = tile.coord.py as usize;

    for row in 0..tile.coord.height as usize {
        let src_off = row * tw * bpp;
        let dst_off = (tpy + row) * stride + tpx * bpp;
        let len = (tw * bpp).min(dst.len().saturating_sub(dst_off));
        dst[dst_off..dst_off + len].copy_from_slice(&src[src_off..src_off + len]);
    }
}

impl SinkRunner for DisplaySinkRunner {
    fn consume(&mut self, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("display_sink:consume");
        match item {
            Item::Tile(tile) => {
                let src: &[u8] = match &tile.data {
                    crate::gpu::Buffer::Cpu(v) => v.as_slice(),
                    crate::gpu::Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
                };
                let mut buf = self.buf.lock().unwrap();
                let w = buf.width;
                copy_tile_to_rgba8(&mut buf.pixels, w, src, &tile);
                buf.dirty = true;
                Ok(())
            }
            _ => Err(Error::internal("expected Tile")),
        }
    }

    fn finish(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
