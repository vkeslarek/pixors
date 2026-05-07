use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::data::buffer::Buffer;
use crate::data::tile::Tile;
use crate::data::tile::TileCoord;
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::common::pixel::meta::PixelMeta;
use crate::stage::{
    DataKind, PortDeclaration, PortGroup, PortSpecification, Processor,
    ProcessorContext, Stage,
};

use crate::debug_stopwatch;

static DL_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static DL_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];

static DL_PORTS: PortSpecification = PortSpecification {
    inputs: PortGroup::Fixed(DL_INPUTS),
    outputs: PortGroup::Fixed(DL_OUTPUTS),
};

const BATCH_SIZE: usize = 16;

struct Pending {
    coord: TileCoord,
    meta: PixelMeta,
    staging: wgpu::Buffer,
    _src_gbuf: Arc<crate::gpu::pool::GpuBuffer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download;

impl Stage for Download {
    fn kind(&self) -> &'static str {
        "download"
    }

    fn ports(&self) -> &'static PortSpecification {
        &DL_PORTS
    }

    fn processor(&self) -> Option<Box<dyn Processor>> {
        Some(Box::new(DownloadProcessor::new()))
    }
}

pub struct DownloadProcessor {
    pending: Vec<Pending>,
    encoder: Option<wgpu::CommandEncoder>,
    flushed_chunks: usize,
}

impl DownloadProcessor {
    pub fn new() -> Self {
        Self {
            pending: vec![],
            encoder: None,
            flushed_chunks: 0,
        }
    }

    fn flush(&mut self, emit: &mut Emitter<Item>, gpu: &crate::gpu::context::GpuContext) -> Result<(), Error> {
        if self.pending.is_empty() {
            return Ok(());
        }

        gpu.scheduler().flush();

        if let Some(encoder) = self.encoder.take() {
            gpu.queue().submit(std::iter::once(encoder.finish()));
        }
        let pending = std::mem::take(&mut self.pending);
        let n = pending.len();

        let (tx, rx) = std::sync::mpsc::channel::<(usize, Result<(), wgpu::BufferAsyncError>)>();
        for (idx, p) in pending.iter().enumerate() {
            let txc = tx.clone();
            p.staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |res| {
                    let _ = txc.send((idx, res));
                });
        }
        drop(tx);
        gpu.device().poll(wgpu::Maintain::Wait);

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
            let mut bytes = {
                let view = p.staging.slice(..).get_mapped_range();
                view.to_vec()
            };
            bytes.truncate(p._src_gbuf.requested_size as usize);
            p.staging.unmap();
            emit.emit(Item::Tile(Tile::new(p.coord, p.meta, Buffer::cpu(bytes))));
        }
        self.flushed_chunks += 1;
        Ok(())
    }
}

impl Processor for DownloadProcessor {
    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("download");
        let tile = ProcessorContext::take_tile(item)?;
        if tile.data.is_cpu() {
            ctx.emit.emit(Item::Tile(tile));
            return Ok(());
        }
        let gpu = ctx.gpu.as_ref().ok_or_else(|| Error::internal("GPU unavailable for download"))?;
        gpu.scheduler().flush();
        let gbuf = tile.data.as_gpu().unwrap().clone();
        let alloc_size = gbuf.allocated_size;
        let staging = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("download-staging"),
            size: alloc_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let encoder = self.encoder.get_or_insert_with(|| {
            gpu.device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("download-batch"),
                })
        });
        encoder.copy_buffer_to_buffer(gbuf.buffer(), 0, &staging, 0, alloc_size);
        self.pending.push(Pending {
            coord: tile.coord,
            meta: tile.meta,
            staging,
            _src_gbuf: gbuf,
        });
        if self.pending.len() >= BATCH_SIZE {
            self.flush(ctx.emit, gpu)?;
        }
        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        if let Some(gpu) = &ctx.gpu {
            self.flush(ctx.emit, gpu)?;
        }
        tracing::info!("[pixors] download: finish — {} chunks flushed total", self.flushed_chunks);
        Ok(())
    }
}
