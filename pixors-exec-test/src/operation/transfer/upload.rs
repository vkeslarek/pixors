use serde::{Deserialize, Serialize};

use crate::data::Tile;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};
use crate::error::Error;
use crate::gpu;
use crate::data::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

static UP_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static UP_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static UP_PORTS: PortSpec = PortSpec { inputs: UP_INPUTS, outputs: UP_OUTPUTS };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload;

impl Stage for Upload {
    fn kind(&self) -> &'static str { "upload" }

    fn ports(&self) -> &'static PortSpec {
        &UP_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(UploadRunner::new()))
    }
}

pub struct UploadRunner;

impl UploadRunner {
    pub fn new() -> Self {
        Self
    }
}

impl CpuKernel for UploadRunner {
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

        let pool = &ctx.scheduler().pool();
        let buf = pool.acquire(size, usage);
        let buf_arc = buf.arc();
        ctx.queue().write_buffer(&buf_arc, 0, bytes);

        let gbuf = GpuBuffer::new(buf_arc, size);
        emit.emit(Item::Tile(Tile::new(tile.coord, tile.meta, Buffer::Gpu(gbuf))));
        Ok(())
    }
}
