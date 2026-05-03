use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::data::Tile;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::graph::runner::OperationRunner;
use crate::data::Device;
use crate::stage::Stage;
use crate::error::Error;
use crate::gpu::{self, GpuContext};
use crate::gpu::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

const BLUR_SPIRV: &[u8] = include_bytes!("../../../kernels/blur.spv");

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
        ctx.scheduler().flush();
        if let Some(encoder) = self.encoder.take() {
            ctx.queue().submit(std::iter::once(encoder.finish()));
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
struct BlurParams {
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

        let pool = &ctx.scheduler().pool();
        let src_buf = pool.acquire(src_size, scratch_usage).arc();
        let dst_buf = pool.acquire(src_size, scratch_usage).arc();
        let out_buf = pool.acquire(out_size, scratch_usage).arc();

        let params = BlurParams {
            width: rw,
            height: rh,
            radius: r,
            _pad: 0,
        };
        let param_buf = pool.acquire(16, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST).arc();
        ctx.queue()
            .write_buffer(&param_buf, 0, bytemuck::bytes_of(&params));

        // Slang SPIR-V layout: group 0 = src@0 + dst@1, group 1 = params@0
        let bgl_g0 = ctx.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bgl_g0"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bgl_g1 = ctx.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bgl_g1"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bg0 = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_bg0"),
            layout: &bgl_g0,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: src_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: dst_buf.as_entire_binding() },
            ],
        });
        let bg1 = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_bg1"),
            layout: &bgl_g1,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: param_buf.as_entire_binding() }],
        });

        let pipeline_layout = ctx.device().create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_layout"),
            bind_group_layouts: &[&bgl_g0, &bgl_g1],
            push_constant_ranges: &[],
        });

        let mut words: Vec<u32> = vec![0u32; BLUR_SPIRV.len() / 4];
        unsafe {
            std::ptr::copy_nonoverlapping(BLUR_SPIRV.as_ptr(), words.as_mut_ptr() as *mut u8, BLUR_SPIRV.len());
        }
        let shader = ctx.device().create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur"),
            source: wgpu::ShaderSource::SpirV(std::borrow::Cow::Owned(words)),
        });
        let pipeline = ctx.device().create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("blur"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "cs_blur",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let encoder = self.encoder.get_or_insert_with(|| {
            ctx.device()
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bg0, &[]);
            pass.set_bind_group(1, &bg1, &[]);
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

        self.keepalive.push(src_buf);
        self.keepalive.push(dst_buf);
        self.keepalive.push(param_buf);
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
