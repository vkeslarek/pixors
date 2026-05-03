use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::container::Tile;
use crate::pipeline::egraph::emitter::Emitter;
use crate::pipeline::egraph::item::Item;
use crate::pipeline::egraph::runner::OperationRunner;
use crate::pipeline::egraph::stage::{Device, Stage};
use crate::error::Error;
use crate::gpu;
use crate::storage::{Buffer, GpuBuffer};
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
        let buffer = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("upload-tile"),
            contents: bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });
        let gbuf = GpuBuffer::new(Arc::new(buffer), bytes.len() as u64);
        emit.emit(Item::Tile(Tile::new(tile.coord, tile.meta, Buffer::Gpu(gbuf))));
        Ok(())
    }
}
