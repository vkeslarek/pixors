use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::data::Tile;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::graph::runner::OperationRunner;
use crate::data::Device;
use crate::stage::{ExecNode, Stage};
use crate::error::Error;
use crate::gpu::{self, GpuContext};
use crate::data::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

const BATCH_SIZE: usize = 16;
const BLUR_SPIRV: &[u8] = include_bytes!("../../../kernels/blur.spv");

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurParams {
    width: u32,
    height: u32,
    radius: u32,
    _pad: u32,
}

struct CachedKind {
    pipeline: Arc<wgpu::ComputePipeline>,
    bgl: Arc<wgpu::BindGroupLayout>,
}

type PipelineCache = HashMap<&'static str, CachedKind>;

// ── Stage ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedGpuKernel {
    pub steps: Vec<ExecNode>,
}

impl Stage for FusedGpuKernel {
    fn kind(&self) -> &'static str {
        "fused_gpu_kernel"
    }
    fn device(&self) -> Device {
        Device::Gpu
    }
    fn allocates_output(&self) -> bool {
        true
    }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(FusedGpuKernelRunner::new(self.steps.clone())))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

pub struct FusedGpuKernelRunner {
    steps: Vec<ExecNode>,
    ctx: Option<Arc<GpuContext>>,
    pipeline_cache: PipelineCache,
    encoder: Option<wgpu::CommandEncoder>,
    keepalive: Vec<Arc<wgpu::Buffer>>,
    in_flight: usize,
    flushed_chunks: usize,
    tile_total: usize,
}

impl FusedGpuKernelRunner {
    pub fn new(steps: Vec<ExecNode>) -> Self {
        Self {
            steps,
            ctx: None,
            pipeline_cache: HashMap::new(),
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

    fn get_or_build_pipeline(
        &mut self,
        device: &wgpu::Device,
        kind: &'static str,
    ) -> Result<&CachedKind, Error> {
        if self.pipeline_cache.contains_key(kind) {
            return Ok(&self.pipeline_cache[kind]);
        }

        let spirv = match kind {
            "blur_kernel_gpu" => BLUR_SPIRV,
            other => return Err(Error::internal(format!("no SPIR-V for kernel kind: {other}"))),
        };

        let mut words: Vec<u32> = vec![0u32; spirv.len() / 4];
        unsafe {
            std::ptr::copy_nonoverlapping(spirv.as_ptr(), words.as_mut_ptr() as *mut u8, spirv.len());
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(kind),
            source: wgpu::ShaderSource::SpirV(std::borrow::Cow::Owned(words)),
        });

        let bgl = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("fused_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            },
        ));

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("fused_layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });

        let pipeline = Arc::new(
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(kind),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: "cs_blur",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            }),
        );

        self.pipeline_cache
            .insert(kind, CachedKind { pipeline, bgl });
        Ok(&self.pipeline_cache[kind])
    }

    fn write_kernel_params(
        &self,
        step: &ExecNode,
        rw: u32,
        rh: u32,
    ) -> Result<Vec<u8>, Error> {
        match step {
            ExecNode::BlurKernelGpu(b) => {
                let params = BlurParams {
                    width: rw,
                    height: rh,
                    radius: b.radius,
                    _pad: 0,
                };
                let mut buf = vec![0u8; 16];
                buf.copy_from_slice(bytemuck::bytes_of(&params));
                Ok(buf)
            }
            _ => Err(Error::internal(format!(
                "unsupported kernel in fused chain: {}",
                step.kind()
            ))),
        }
    }

    fn submit_chunk(&mut self) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        if let Some(encoder) = self.encoder.take() {
            ctx.queue().submit(std::iter::once(encoder.finish()));
        }
        self.keepalive.clear();
        if self.in_flight > 0 {
            tracing::debug!(
                "[pixors] fused_gpu_kernel: submitted chunk of {} tiles",
                self.in_flight
            );
            self.flushed_chunks += 1;
        }
        self.in_flight = 0;
    }
}

impl OperationRunner for FusedGpuKernelRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("fused_gpu_kernel");
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("FusedGpuKernel expected Neighborhood")),
        };
        let ctx = self.ctx()?;
        let device = ctx.device();
        let n = self.steps.len();

        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let bpp = 4u32;

        let max_r = self
            .steps
            .iter()
            .map(|s| match s {
                ExecNode::BlurKernelGpu(b) => b.radius,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let rw = cw + 2 * max_r;
        let rh = ch + 2 * max_r;
        let rox = cx.saturating_sub(max_r);
        let roy = cy.saturating_sub(max_r);
        let padded_size = (rw as u64) * (rh as u64) * (bpp as u64);
        let out_size = (cw as u64) * (ch as u64) * (bpp as u64);

        let scratch_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;

        let pool = ctx.scheduler().pool();

        let mut bufs: Vec<Arc<wgpu::Buffer>> = (0..=n)
            .map(|i| {
                let sz = if i == n { out_size } else { padded_size };
                pool.acquire(sz, scratch_usage).arc()
            })
            .collect();

        let param_data: Vec<Vec<u8>> = self
            .steps
            .iter()
            .map(|s| self.write_kernel_params(s, rw, rh))
            .collect::<Result<_, _>>()?;

        let param_bufs: Vec<Arc<wgpu::Buffer>> = param_data
            .iter()
            .map(|data| {
                let pb = pool
                    .acquire(data.len() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST)
                    .arc();
                ctx.queue().write_buffer(&pb, 0, data);
                pb
            })
            .collect();

        // Pre-build pipelines for each unique kind
        let kinds: Vec<&'static str> = self.steps.iter().map(|s| s.kind()).collect();
        for &kind in &kinds {
            self.get_or_build_pipeline(device, kind)?;
        }

        let encoder = self.encoder.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fused-gpu-batch"),
            })
        });

        for tile in &nbhd.tiles {
            let gbuf = tile
                .data
                .as_gpu()
                .ok_or_else(|| Error::internal("FusedGpuKernel: tile not on GPU"))?;
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
                encoder.copy_buffer_to_buffer(&gbuf.buffer, src_off, &bufs[0], dst_off, len);
            }
        }

        let gx = rw.div_ceil(8);
        let gy = rh.div_ceil(8);
        for i in 0..n {
            let kind = kinds[i];
            let cached = &self.pipeline_cache[kind];

            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fused_bg"),
                layout: &cached.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: param_bufs[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: bufs[i].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: bufs[i + 1].as_entire_binding(),
                    },
                ],
            });

            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&format!("fused-pass-{i}")),
                timestamp_writes: None,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }

        let off_x = cx - rox;
        let off_y = cy - roy;
        let out_buf = pool.acquire(out_size, scratch_usage).arc();
        for row in 0..ch {
            let src_off = (((off_y + row) * rw + off_x) * bpp) as u64;
            let dst_off = ((row * cw) * bpp) as u64;
            let len = (cw * bpp) as u64;
            encoder.copy_buffer_to_buffer(&bufs[n], src_off, &out_buf, dst_off, len);
        }

        for b in &bufs {
            self.keepalive.push(b.clone());
        }
        for pb in &param_bufs {
            self.keepalive.push(pb.clone());
        }
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
            "[pixors] fused_gpu_kernel: total {} tiles in {} chunks",
            self.tile_total,
            self.flushed_chunks
        );
        Ok(())
    }
}
