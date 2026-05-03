use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::container::Tile;
use crate::pipeline::exec_graph::emitter::Emitter;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::OperationRunner;
use crate::pipeline::exec::{Device, Stage};
use crate::error::Error;
use crate::gpu::{self, GpuContext};
use crate::gpu::kernels::blur as gpu_blur;
use crate::gpu::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

const BATCH_SIZE: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlurKernelGpu {
    pub radius: u32,
}

impl Stage for BlurKernelGpu {
    fn kind(&self) -> &'static str {
        "blur_kernel_gpu"
    }
    fn device(&self) -> Device {
        Device::Gpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(BlurKernelGpuRunner::new(self.radius)))
    }
}

pub struct BlurKernelGpuRunner {
    radius: u32,
    ctx: Option<Arc<GpuContext>>,
    encoder: Option<wgpu::CommandEncoder>,
    keepalive: Vec<Arc<wgpu::Buffer>>,
    in_flight: usize,
    flushed_chunks: usize,
    tile_total: usize,
}

impl BlurKernelGpuRunner {
    pub fn new(radius: u32) -> Self {
        Self {
            radius,
            ctx: None,
            encoder: None,
            keepalive: vec![],
            in_flight: 0,
            flushed_chunks: 0,
            tile_total: 0,
        }
    }

    fn ctx(&mut self) -> Result<Arc<GpuContext>, Error> {
        if let Some(c) = &self.ctx {
            return Ok(c.clone());
        }
        let c = gpu::try_init().ok_or_else(|| Error::internal("GPU unavailable"))?;
        self.ctx = Some(c.clone());
        Ok(c)
    }

    fn submit_chunk(&mut self) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        if let Some(encoder) = self.encoder.take() {
            ctx.queue.submit(std::iter::once(encoder.finish()));
        }
        self.keepalive.clear();
        if self.in_flight > 0 {
            tracing::debug!(
                "[pixors] blur_kernel_gpu: submitted chunk of {} tiles",
                self.in_flight
            );
            self.flushed_chunks += 1;
        }
        self.in_flight = 0;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    radius: u32,
    _pad: u32,
}

impl OperationRunner for BlurKernelGpuRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("blur_kernel_gpu");
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("BlurKernelGpu expected Neighborhood")),
        };
        let ctx = self.ctx()?;

        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let r = self.radius;
        let bpp = 4u32;

        let rw = cw + 2 * r;
        let rh = ch + 2 * r;
        let rox = cx.saturating_sub(r);
        let roy = cy.saturating_sub(r);
        let src_size = (rw as u64) * (rh as u64) * (bpp as u64);
        let out_size = (cw as u64) * (ch as u64) * (bpp as u64);

        let scratch_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;

        let src_buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur-gpu-src"),
            size: src_size,
            usage: scratch_usage,
            mapped_at_creation: false,
        });
        let dst_buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur-gpu-dst"),
            size: src_size,
            usage: scratch_usage,
            mapped_at_creation: false,
        });
        let out_buf = Arc::new(ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur-gpu-out"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }));

        let params = Params {
            width: rw,
            height: rh,
            radius: r,
            _pad: 0,
        };
        let params_buf = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur-gpu-params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let pipeline = gpu_blur::get_or_init(&ctx);
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur-gpu-bg"),
            layout: &pipeline.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: src_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dst_buf.as_entire_binding(),
                },
            ],
        });

        let encoder = self.encoder.get_or_insert_with(|| {
            ctx.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("blur-gpu-batch"),
                })
        });

        for tile in &nbhd.tiles {
            let gbuf = tile
                .data
                .as_gpu()
                .ok_or_else(|| Error::internal("BlurKernelGpu: tile not on GPU"))?;
            let tw = tile.coord.width;
            let tpx = tile.coord.px;
            let tpy = tile.coord.py;

            let x0 = rox.max(tpx);
            let y0 = roy.max(tpy);
            let x1 = (rox + rw).min(tpx + tile.coord.width);
            let y1 = (roy + rh).min(tpy + tile.coord.height);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let copy_w = x1 - x0;
            for abs_y in y0..y1 {
                let src_row = abs_y - tpy;
                let src_col = x0 - tpx;
                let dst_row = abs_y - roy;
                let dst_col = x0 - rox;
                let src_off = ((src_row * tw + src_col) * bpp) as u64;
                let dst_off = ((dst_row * rw + dst_col) * bpp) as u64;
                let len = (copy_w * bpp) as u64;
                encoder.copy_buffer_to_buffer(&gbuf.buffer, src_off, &src_buf, dst_off, len);
            }
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("blur-gpu-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let gx = rw.div_ceil(8);
            let gy = rh.div_ceil(8);
            pass.dispatch_workgroups(gx, gy, 1);
        }

        let off_x = cx - rox;
        let off_y = cy - roy;
        for row in 0..ch {
            let src_off = (((off_y + row) * rw + off_x) * bpp) as u64;
            let dst_off = ((row * cw) * bpp) as u64;
            let len = (cw * bpp) as u64;
            encoder.copy_buffer_to_buffer(&dst_buf, src_off, &out_buf, dst_off, len);
        }

        self.keepalive.push(Arc::new(src_buf));
        self.keepalive.push(Arc::new(dst_buf));
        self.keepalive.push(Arc::new(params_buf));
        self.keepalive.push(out_buf.clone());

        let gbuf = GpuBuffer::new(out_buf, out_size);
        emit.emit(Item::Tile(Tile::new(
            nbhd.center,
            nbhd.meta,
            Buffer::Gpu(gbuf),
        )));

        self.in_flight += 1;
        self.tile_total += 1;
        if self.in_flight >= BATCH_SIZE {
            self.submit_chunk();
        }
        Ok(())
    }

    fn finish(&mut self, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        self.submit_chunk();
        tracing::debug!(
            "[pixors] blur_kernel_gpu: total {} tiles in {} chunks (BATCH_SIZE={})",
            self.tile_total,
            self.flushed_chunks,
            BATCH_SIZE
        );
        Ok(())
    }
}
