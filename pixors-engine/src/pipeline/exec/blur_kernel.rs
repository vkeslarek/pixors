use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::container::Tile;
use crate::pipeline::egraph::emitter::Emitter;
use crate::pipeline::egraph::item::Item;
use crate::pipeline::egraph::runner::OperationRunner;
use crate::pipeline::egraph::stage::{Device, Stage};
use crate::error::Error;
use crate::gpu::{self, GpuContext};
use crate::gpu::kernels::blur as gpu_blur;
use crate::storage::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

// ── CPU ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlurKernel {
    pub radius: u32,
}

impl Stage for BlurKernel {
    fn kind(&self) -> &'static str {
        "blur_kernel"
    }
    fn device(&self) -> Device {
        Device::Cpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(BlurKernelRunner::new(self.radius)))
    }
}

pub struct BlurKernelRunner {
    radius: u32,
}

impl BlurKernelRunner {
    pub fn new(radius: u32) -> Self {
        Self { radius }
    }
}

impl OperationRunner for BlurKernelRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("blur_kernel");
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("expected Neighborhood")),
        };

        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let r = self.radius;
        let bpp = 4usize;

        let rw = (cw + 2 * r) as usize;
        let rh = (ch + 2 * r) as usize;
        let rox = cx.saturating_sub(r);
        let roy = cy.saturating_sub(r);

        let mut src = vec![0u8; rw * rh * bpp];

        for tile in &nbhd.tiles {
            let tile_data: &[u8] = match &tile.data {
                Buffer::Cpu(v) => v.as_slice(),
                Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
            };
            let tw = tile.coord.width as usize;
            let tpx = tile.coord.px;
            let tpy = tile.coord.py;

            let x0 = rox.max(tpx);
            let y0 = roy.max(tpy);
            let x1 = (rox + rw as u32).min(tpx + tile.coord.width);
            let y1 = (roy + rh as u32).min(tpy + tile.coord.height);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }

            let copy_w = (x1 - x0) as usize;
            for abs_y in y0..y1 {
                let src_row = (abs_y - tpy) as usize;
                let src_col = (x0 - tpx) as usize;
                let dst_row = (abs_y - roy) as usize;
                let dst_col = (x0 - rox) as usize;

                let src_off = (src_row * tw + src_col) * bpp;
                let dst_off = (dst_row * rw + dst_col) * bpp;
                let len = copy_w * bpp;

                if src_off + len > tile_data.len() || dst_off + len > src.len() {
                    continue;
                }
                src[dst_off..dst_off + len].copy_from_slice(&tile_data[src_off..src_off + len]);
            }
        }

        let blurred = box_blur_rgba8(&src, rw, rh, r as usize);

        let cw_u = cw as usize;
        let ch_u = ch as usize;
        let off_x = (cx - rox) as usize;
        let off_y = (cy - roy) as usize;
        let mut tile_data = Vec::with_capacity(cw_u * ch_u * bpp);
        for y in 0..ch_u {
            let row_off = ((off_y + y) * rw + off_x) * bpp;
            tile_data.extend_from_slice(&blurred[row_off..row_off + cw_u * bpp]);
        }

        emit.emit(Item::Tile(Tile::new(
            nbhd.center,
            nbhd.meta,
            Buffer::cpu(tile_data),
        )));
        Ok(())
    }
}

// ── GPU ────────────────────────────────────────────────────────────────────

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

// ── Shared helpers ─────────────────────────────────────────────────────────

fn box_blur_rgba8(data: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if r == 0 {
        return data.to_vec();
    }

    let stride = w * 4;
    let hpass = blur_axis(data, h, stride, w, 4, r);
    blur_axis(&hpass, w, 4, h, stride, r)
}

fn blur_axis(
    data: &[u8],
    lines: usize,
    line_step: usize,
    axis_len: usize,
    step: usize,
    r: usize,
) -> Vec<u8> {
    let mut dst = vec![0u8; data.len()];

    for line in 0..lines {
        let line_origin = line * line_step;
        let mut sum = [0u32; 4];
        let mut count = 0u32;

        let initial_end = r.min(axis_len - 1);
        for i in 0..=initial_end {
            let off = line_origin + i * step;
            for c in 0..4 {
                sum[c] += data[off + c] as u32;
            }
            count += 1;
        }

        for i in 0..axis_len {
            if i > 0 {
                let new_i = i + r;
                if new_i < axis_len {
                    let off = line_origin + new_i * step;
                    for c in 0..4 {
                        sum[c] += data[off + c] as u32;
                    }
                    count += 1;
                }
                if i > r {
                    let old_i = i - r - 1;
                    let off = line_origin + old_i * step;
                    for c in 0..4 {
                        sum[c] -= data[off + c] as u32;
                    }
                    count -= 1;
                }
            }
            let dst_off = line_origin + i * step;
            for c in 0..4 {
                dst[dst_off + c] = (sum[c] / count) as u8;
            }
        }
    }

    dst
}
