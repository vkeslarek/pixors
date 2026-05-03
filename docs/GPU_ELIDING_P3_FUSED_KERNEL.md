# GPU Eliding — Phase 3: FusedBlurKernelGpu Stage + Runner

## New File: `pixors-engine/src/pipeline/exec/blur_kernel/fused.rs`

This replaces two consecutive `BlurKernelGpuRunner`s with a single runner that:
1. Allocates N+1 storage buffers (src, tmp_0…tmp_{N-2}, dst)
2. Generates/caches a WGSL shader with N entry points
3. Records N sequential compute passes in one `CommandEncoder`
4. Submits once per batch of tiles

### Full code

```rust
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::container::Tile;
use crate::pipeline::exec_graph::emitter::Emitter;
use crate::pipeline::exec_graph::item::Item;
use crate::pipeline::exec_graph::runner::OperationRunner;
use crate::pipeline::exec::{Device, Stage};
use crate::error::Error;
use crate::gpu::{self, GpuContext};
use crate::gpu::{Buffer, GpuBuffer};
use crate::debug_stopwatch;

// Caches one `(BGL, Vec<Pipeline>)` entry per unique radii combination.
// Key = hash of radii slice.
type PipelineCache = HashMap<u64, FusedPipelineSet>;

struct FusedPipelineSet {
    bgl: Arc<wgpu::BindGroupLayout>,
    pipelines: Vec<Arc<wgpu::ComputePipeline>>, // one per pass
    num_passes: usize,
}

const BATCH_SIZE: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurParams {
    width:  u32,
    height: u32,
    radius: u32,
    _pad:   u32,
}

// ── Stage definition ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedBlurKernelGpu {
    pub radii: Vec<u32>,
}

impl Stage for FusedBlurKernelGpu {
    fn kind(&self) -> &'static str { "fused_blur_kernel_gpu" }
    fn device(&self) -> Device { Device::Gpu }
    fn allocates_output(&self) -> bool { true }
    fn op_runner(&self) -> Result<Box<dyn OperationRunner>, Error> {
        Ok(Box::new(FusedBlurKernelGpuRunner::new(self.radii.clone())))
    }
}

// ── Runner ───────────────────────────────────────────────────────────────────

pub struct FusedBlurKernelGpuRunner {
    radii: Vec<u32>,
    ctx: Option<Arc<GpuContext>>,
    pipeline_cache: PipelineCache,
    encoder: Option<wgpu::CommandEncoder>,
    keepalive: Vec<Arc<wgpu::Buffer>>,
    in_flight: usize,
    flushed_chunks: usize,
    tile_total: usize,
}

impl FusedBlurKernelGpuRunner {
    pub fn new(radii: Vec<u32>) -> Self {
        Self {
            radii,
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
        if let Some(c) = &self.ctx { return Ok(c.clone()); }
        let c = gpu::try_init().ok_or_else(|| Error::internal("GPU unavailable"))?;
        self.ctx = Some(c.clone());
        Ok(c)
    }

    fn get_or_build_pipelines(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<&FusedPipelineSet, Error> {
        let key = hash_radii(&self.radii);
        if self.pipeline_cache.contains_key(&key) {
            return Ok(&self.pipeline_cache[&key]);
        }

        let n = self.radii.len();
        let shader_data = pixors_shader::codegen::gen_fused_blur(&self.radii);
        let bgl = pixors_shader::scheduler::build_fused_blur_bgl(device, n);

        // Compile the WGSL once; extract each entry point as its own pipeline.
        let shader_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fused_blur_wgsl"),
            source: wgpu::ShaderSource::Wgsl(
                std::borrow::Cow::Owned(shader_data.wgsl),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fused_blur_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipelines: Vec<Arc<wgpu::ComputePipeline>> = shader_data
            .entry_points
            .iter()
            .map(|ep| {
                Arc::new(device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(ep.as_str()),
                    layout: Some(&layout),
                    module: &shader_mod,
                    entry_point: Some(ep.as_str()),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                }))
            })
            .collect();

        self.pipeline_cache.insert(
            key,
            FusedPipelineSet { bgl, pipelines, num_passes: n },
        );
        Ok(&self.pipeline_cache[&key])
    }

    fn submit_chunk(&mut self) {
        let Some(ctx) = self.ctx.clone() else { return; };
        if let Some(encoder) = self.encoder.take() {
            ctx.queue().submit(std::iter::once(encoder.finish()));
        }
        self.keepalive.clear();
        if self.in_flight > 0 {
            tracing::debug!(
                "[pixors] fused_blur_gpu: submitted chunk of {} tiles",
                self.in_flight
            );
            self.flushed_chunks += 1;
        }
        self.in_flight = 0;
    }
}

impl OperationRunner for FusedBlurKernelGpuRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("fused_blur_kernel_gpu");
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("FusedBlurKernelGpu expected Neighborhood")),
        };
        let ctx = self.ctx()?;
        let device = ctx.device();

        // Build/cache pipelines on first tile.
        let n = self.radii.len();
        // Clone to avoid borrow conflict with self.encoder below.
        let (bgl_arc, pipelines) = {
            let ps = self.get_or_build_pipelines(device)?;
            (ps.bgl.clone(), ps.pipelines.clone())
        };

        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let bpp = 4u32;

        // For fused blur chain: the padded region grows by r per pass.
        // Each pass needs a neighborhood radius equal to its own blur radius.
        // We use the maximum radius to size all buffers uniformly for simplicity.
        let max_r = *self.radii.iter().max().unwrap_or(&0);
        let rw = cw + 2 * max_r;
        let rh = ch + 2 * max_r;
        let rox = cx.saturating_sub(max_r);
        let roy = cy.saturating_sub(max_r);

        let padded_size = (rw as u64) * (rh as u64) * (bpp as u64);
        let out_size    = (cw as u64) * (ch as u64) * (bpp as u64);

        let scratch_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;

        let pool = ctx.scheduler().pool();

        // Allocate N+1 storage buffers: src, tmp_0..tmp_{n-2}, dst.
        // All padded-size except the final output which is center-size.
        let mut bufs: Vec<Arc<wgpu::Buffer>> = (0..=n)
            .map(|i| {
                let sz = if i == n { out_size } else { padded_size };
                pool.acquire(sz, scratch_usage).arc()
            })
            .collect();
        // bufs[0] = src, bufs[1..n-1] = tmp, bufs[n] = dst

        // Write uniform param buffers (one per pass).
        let param_bufs: Vec<Arc<wgpu::Buffer>> = self.radii.iter().enumerate().map(|(i, &r)| {
            let params = BlurParams { width: rw, height: rh, radius: r, _pad: 0 };
            let pb = pool.acquire(16, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST).arc();
            ctx.queue().write_buffer(&pb, 0, bytemuck::bytes_of(&params));
            pb
        }).collect();

        // Build bind group.
        let mut bg_entries: Vec<wgpu::BindGroupEntry> = Vec::new();
        // Uniform bindings 0..n-1
        for (i, pb) in param_bufs.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: i as u32,
                resource: pb.as_entire_binding(),
            });
        }
        // Storage bindings n..2n: src(read), tmp*(rw), dst(rw)
        for (k, buf) in bufs.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: (n + k) as u32,
                resource: buf.as_entire_binding(),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fused_blur_bg"),
            layout: &bgl_arc,
            entries: &bg_entries,
        });

        let encoder = self.encoder.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fused-blur-batch"),
            })
        });

        // Copy neighborhood tiles into src_buf (bufs[0]).
        let src_buf = &bufs[0];
        for tile in &nbhd.tiles {
            let gbuf = tile.data.as_gpu()
                .ok_or_else(|| Error::internal("FusedBlurKernelGpu: tile not on GPU"))?;
            let tw = tile.coord.width;
            let tpx = tile.coord.px;
            let tpy = tile.coord.py;
            let x0 = rox.max(tpx);
            let y0 = roy.max(tpy);
            let x1 = (rox + rw).min(tpx + tile.coord.width);
            let y1 = (roy + rh).min(tpy + tile.coord.height);
            if x1 <= x0 || y1 <= y0 { continue; }
            let copy_w = x1 - x0;
            for abs_y in y0..y1 {
                let src_row = abs_y - tpy;
                let src_col = x0 - tpx;
                let dst_row = abs_y - roy;
                let dst_col = x0 - rox;
                let src_off = ((src_row * tw + src_col) * bpp) as u64;
                let dst_off = ((dst_row * rw + dst_col) * bpp) as u64;
                let len = (copy_w * bpp) as u64;
                encoder.copy_buffer_to_buffer(&gbuf.buffer, src_off, src_buf, dst_off, len);
            }
        }

        // Dispatch N passes.
        let gx = rw.div_ceil(8);
        let gy = rh.div_ceil(8);
        for i in 0..n {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&format!("fused-blur-pass-{i}")),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipelines[i]);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }

        // Crop dst_buf (bufs[n]) from padded output to center region.
        let off_x = cx - rox;
        let off_y = cy - roy;
        let out_buf = pool.acquire(out_size, scratch_usage).arc();
        for row in 0..ch {
            let src_off = (((off_y + row) * rw + off_x) * bpp) as u64;
            let dst_off = ((row * cw) * bpp) as u64;
            let len = (cw * bpp) as u64;
            encoder.copy_buffer_to_buffer(&bufs[n], src_off, &out_buf, dst_off, len);
        }

        // Keepalive everything until submit.
        for b in &bufs { self.keepalive.push(b.clone()); }
        for pb in &param_bufs { self.keepalive.push(pb.clone()); }
        self.keepalive.push(out_buf.clone());

        let gbuf = GpuBuffer::new(out_buf, out_size);
        emit.emit(Item::Tile(Tile::new(nbhd.center, nbhd.meta, Buffer::Gpu(gbuf))));

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
            "[pixors] fused_blur_gpu: total {} tiles in {} chunks",
            self.tile_total,
            self.flushed_chunks
        );
        Ok(())
    }
}

fn hash_radii(radii: &[u32]) -> u64 {
    let mut h = DefaultHasher::new();
    radii.hash(&mut h);
    h.finish()
}
```

## Register `FusedBlurKernelGpu` in `exec/blur_kernel/mod.rs`

Current `mod.rs`:
```rust
pub mod cpu;
pub mod gpu;

pub use cpu::{BlurKernel, BlurKernelRunner};
pub use gpu::{BlurKernelGpu, BlurKernelGpuRunner};
```

Add:
```rust
pub mod fused;
pub use fused::{FusedBlurKernelGpu, FusedBlurKernelGpuRunner};
```

## Register `FusedBlurKernelGpu` in `exec/mod.rs`

Current `ExecNode` enum includes `BlurKernelGpu`. Add `FusedBlurKernelGpu`:

```rust
// In imports:
pub use blur_kernel::{BlurKernel, BlurKernelGpu, BlurKernelGpuRunner, BlurKernelRunner,
                      FusedBlurKernelGpu, FusedBlurKernelGpuRunner};

// In ExecNode enum:
#[enum_dispatch(Stage)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecNode {
    FileDecoder,
    ScanLineAccumulator,
    ColorConvert,
    NeighborhoodAgg,
    BlurKernel,
    BlurKernelGpu,
    FusedBlurKernelGpu,   // ← add this
    Upload,
    Download,
    CacheReader,
    CacheWriter,
    PngEncoder,
    TileToScanline,
    TileSink,
}
```

## Verify

```bash
cargo check -p pixors-engine
```

No errors should remain after P1 + P3. Run:
```bash
cargo test -p pixors-engine -- blur
```
