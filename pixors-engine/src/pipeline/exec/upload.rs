use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::container::Tile;
use crate::pipeline::exec_graph::emitter::Emitter;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::OperationRunner;
use super::{Device, Stage};
use crate::error::Error;
use crate::gpu;
use crate::gpu::scheduler::Scheduler;
use crate::gpu::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload;

impl Stage for Upload {
    fn kind(&self) -> &'static str { "upload" }
    fn device(&self) -> Device { Device::Gpu }
    fn allocates_output(&self) -> bool { true }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(UploadRunner::new()))
    }
}

/// CPU → GPU. Receives `Tile`s with `Buffer::Cpu`, uploads to a fresh wgpu
/// storage buffer, emits `Tile`s with `Buffer::Gpu`. Tiles already on GPU
/// pass through unchanged.
pub struct UploadRunner;

impl UploadRunner {
    pub fn new() -> Self {
        Self
    }
}

impl OperationRunner for UploadRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("upload");
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("Upload expected Tile")),
        };
        if tile.data.is_gpu() {
            emit.emit(Item::Tile(tile));
            return Ok(());
        }
        let ctx = gpu::try_init()
            .ok_or_else(|| Error::internal("GPU unavailable but Upload was scheduled"))?;
        let bytes: &[u8] = tile.data.as_cpu_slice().unwrap();
        let size = bytes.len() as u64;
        let usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;

        // Use pool for buffer allocation
        let _ = Scheduler::init(ctx.clone());
        let pool = &Scheduler::global().pool();
        let mut buf = pool.acquire(size, usage);
        let buf_arc = buf.arc();
        ctx.queue.write_buffer(&buf_arc, 0, bytes);

        let gbuf = GpuBuffer::new(buf_arc, size);
        emit.emit(Item::Tile(Tile::new(tile.coord, tile.meta, Buffer::Gpu(gbuf))));
        Ok(())
    }
}
