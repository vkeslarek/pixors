use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::data::Tile;
use crate::model::pixel::meta::PixelMeta;
use crate::data::tile::TileCoord;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};
use crate::error::Error;
use crate::gpu::{self, GpuContext};
use crate::data::Buffer;
use crate::debug_stopwatch;

static DL_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static DL_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static DL_PORTS: PortSpec = PortSpec { inputs: DL_INPUTS, outputs: DL_OUTPUTS };

/// How many tiles to accumulate before flushing (1 submit + 1
/// `device.poll(Wait)` per chunk). Caps peak staging memory.
const BATCH_SIZE: usize = 16;

struct Pending {
    coord: TileCoord,
    meta: PixelMeta,
    staging: wgpu::Buffer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download;

impl Stage for Download {
    fn kind(&self) -> &'static str { "download" }

    fn ports(&self) -> &'static PortSpec {
        &DL_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: false,
        }
    }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(DownloadRunner::new()))
    }
}

/// GPU → CPU. Accumulates up to `BATCH_SIZE` tiles, then issues a single
/// submit + poll for the chunk. Was previously per-tile poll, the dominant
/// pipeline stall.
pub struct DownloadRunner {
    pending: Vec<Pending>,
    encoder: Option<wgpu::CommandEncoder>,
    ctx: Option<Arc<GpuContext>>,
    flushed_chunks: usize,
}

impl DownloadRunner {
    pub fn new() -> Self {
        Self {
            pending: vec![],
            encoder: None,
            ctx: None,
            flushed_chunks: 0,
        }
    }

    fn ctx(&mut self) -> Result<Arc<GpuContext>, Error> {
        if let Some(c) = &self.ctx {
            return Ok(c.clone());
        }
        let c = gpu::try_init()
            .ok_or_else(|| Error::internal("GPU unavailable but Download was scheduled"))?;
        self.ctx = Some(c.clone());
        Ok(c)
    }

    fn flush(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let ctx = self
            .ctx
            .as_ref()
            .ok_or_else(|| Error::internal("download flush without ctx"))?
            .clone();
        if let Some(encoder) = self.encoder.take() {
            ctx.queue().submit(std::iter::once(encoder.finish()));
        }
        let pending = std::mem::take(&mut self.pending);
        let n = pending.len();

        let (tx, rx) = std::sync::mpsc::channel::<(usize, Result<(), wgpu::BufferAsyncError>)>();
        for (idx, p) in pending.iter().enumerate() {
            let txc = tx.clone();
            p.staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                let _ = txc.send((idx, res));
            });
        }
        drop(tx);
        ctx.device().poll(wgpu::Maintain::Wait);

        let mut errors: Vec<Option<Result<(), wgpu::BufferAsyncError>>> =
            (0..n).map(|_| None).collect();
        for _ in 0..n {
            let (idx, r) = rx.recv().map_err(|_| Error::internal("download recv"))?;
            errors[idx] = Some(r);
        }
        for (i, p) in pending.into_iter().enumerate() {
            errors[i]
                .take()
                .unwrap()
                .map_err(|e| Error::internal(format!("map_async: {e:?}")))?;
            let bytes = {
                let view = p.staging.slice(..).get_mapped_range();
                view.to_vec()
            };
            p.staging.unmap();
            emit.emit(Item::Tile(Tile::new(p.coord, p.meta, Buffer::cpu(bytes))));
        }
        self.flushed_chunks += 1;
        tracing::debug!("[pixors] download: flushed chunk of {} tiles", n);
        Ok(())
    }
}

impl CpuKernel for DownloadRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("download");
        let tile = match item {
            Item::Tile(t) => t,
            _ => return Err(Error::internal("Download expected Tile")),
        };
        if tile.data.is_cpu() {
            emit.emit(Item::Tile(tile));
            return Ok(());
        }
        let ctx = self.ctx()?;
        // Flush scheduler so any pending compute dispatches are submitted
        // before the copy_buffer_to_buffer that reads their output.
        ctx.scheduler().flush();
        let gbuf = tile.data.as_gpu().unwrap().clone();
        let size = gbuf.size;
        let staging = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("download-staging"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let encoder = self.encoder.get_or_insert_with(|| {
            ctx.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("download-batch"),
            })
        });
        encoder.copy_buffer_to_buffer(&gbuf.buffer, 0, &staging, 0, size);
        self.pending.push(Pending {
            coord: tile.coord,
            meta: tile.meta,
            staging,
        });
        if self.pending.len() >= BATCH_SIZE {
            self.flush(emit)?;
        }
        Ok(())
    }

    fn finish(&mut self, emit: &mut Emitter<Item>) -> Result<(), Error> {
        self.flush(emit)?;
        tracing::debug!(
            "[pixors] download: total {} chunks (BATCH_SIZE={})",
            self.flushed_chunks, BATCH_SIZE
        );
        Ok(())
    }
}
