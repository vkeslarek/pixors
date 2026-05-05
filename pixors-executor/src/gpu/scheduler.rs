use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};
use crate::data::buffer::Buffer;
use crate::data::tile::Tile;
use crate::error::Error;
use crate::gpu::kernel::{GpuKernel, KernelSignature};
use crate::gpu::pool::BufferPool;

const BATCH_SIZE: usize = 16;

struct CachedPipeline {
    pipeline: Arc<wgpu::ComputePipeline>,
    bgls: Vec<Arc<wgpu::BindGroupLayout>>,
    has_params: bool,
}

/// Reference-counted GPU buffer with known byte size.
#[derive(Clone, Debug)]
pub struct GpuBuffer {
    pub buffer: Arc<wgpu::Buffer>,
    pub size: u64,
}

impl GpuBuffer {
    pub fn new(buffer: Arc<wgpu::Buffer>, size: u64) -> Self {
        Self { buffer, size }
    }
}

pub struct Scheduler {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pool: Arc<BufferPool>,
    cache: Mutex<HashMap<u64, CachedPipeline>>,
    encoder: Mutex<Option<wgpu::CommandEncoder>>,
    keepalive: Mutex<Vec<Arc<wgpu::Buffer>>>,
    in_flight: Mutex<usize>,
}

impl Scheduler {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Arc<Self> {
        let pool = BufferPool::new(device.clone());
        Arc::new(Self {
            device,
            queue,
            pool,
            cache: Mutex::new(HashMap::new()),
            encoder: Mutex::new(None),
            keepalive: Mutex::new(Vec::new()),
            in_flight: Mutex::new(0),
        })
    }

    pub fn dispatch_one(
        &self,
        kernel: &dyn GpuKernel,
        inputs: &[&GpuBuffer],
        out_size: u64,
        dispatch_x: u32,
        dispatch_y: u32,
    ) -> Result<Arc<wgpu::Buffer>, String> {
        let sig = kernel.signature();
        let sig_hash = hash_signature(sig);

        let cached = {
            let mut cache = self.cache.lock().unwrap();
            cache.entry(sig_hash).or_insert_with(|| {
                build_pipeline(&self.device, sig).expect("failed to build pipeline")
            });
            cache.get(&sig_hash).unwrap().clone_arcs()
        };

        let out_buf_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let mut out_buf = self.pool.acquire(out_size, out_buf_usage);

        let param_size = compute_param_size(sig).max(16);
        let param_buf = self.pool.acquire(
            param_size,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let mut params = vec![0u8; param_size as usize];
        kernel.write_params(&mut params);
        self.queue.write_buffer(&param_buf, 0, &params);

        let (bg0, bg1) = build_bind_groups(
            &self.device,
            &cached.bgls,
            cached.has_params,
            inputs,
            &param_buf,
            &out_buf,
        )?;

        {
            let mut enc_guard = self.encoder.lock().unwrap();
            let encoder = enc_guard.get_or_insert_with(|| {
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("scheduler_batch"),
                    })
            });

            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("kernel_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(0, &bg0, &[]);
            if let Some(ref bg1) = bg1 {
                pass.set_bind_group(1, bg1, &[]);
            }
            pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }

        let out_arc = out_buf.arc();
        self.keepalive.lock().unwrap().push(out_arc.clone());
        self.keepalive.lock().unwrap().push(param_buf.arc());

        let mut inflight = self.in_flight.lock().unwrap();
        *inflight += 1;
        if *inflight >= BATCH_SIZE {
            drop(inflight);
            self.flush();
        }

        Ok(out_arc)
    }

    pub fn flush(&self) {
        let mut enc_guard = self.encoder.lock().unwrap();
        if let Some(encoder) = enc_guard.take() {
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        self.keepalive.lock().unwrap().clear();
        *self.in_flight.lock().unwrap() = 0;
    }

    pub fn pool(&self) -> &Arc<BufferPool> {
        &self.pool
    }

    /// Upload raw bytes into a `STORAGE | COPY_SRC | COPY_DST` GPU buffer.
    pub fn upload_bytes(&self, data: &[u8]) -> GpuBuffer {
        let size = data.len() as u64;
        let usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let pool_buf = self.pool.acquire(size, usage);
        let arc = pool_buf.arc();
        self.queue.write_buffer(&arc, 0, data);
        GpuBuffer::new(arc, size)
    }

    /// Download a GPU buffer to CPU bytes.
    /// Caller must call `flush()` first to ensure pending dispatches are submitted.
    pub fn download_buffer(&self, gbuf: &GpuBuffer) -> Result<Vec<u8>, Error> {
        let size = gbuf.size;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sched-staging"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sched-download"),
        });
        enc.copy_buffer_to_buffer(&gbuf.buffer, 0, &staging, 0, size);
        self.queue.submit(std::iter::once(enc.finish()));

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| Error::internal("download recv"))?
            .map_err(|e| Error::internal(format!("download map: {e:?}")))?;

        let bytes = staging.slice(..).get_mapped_range().to_vec();
        staging.unmap();
        Ok(bytes)
    }

    /// Download a tile to CPU if GPU-backed. No-op if already CPU.
    /// Caller must call `flush()` first.
    pub fn download_tile(&self, tile: &Tile) -> Result<Tile, Error> {
        match &tile.data {
            Buffer::Cpu(_) => Ok(tile.clone()),
            Buffer::Gpu(gbuf) => {
                let bytes = self.download_buffer(gbuf)?;
                Ok(Tile::new(tile.coord, tile.meta, Buffer::cpu(bytes)))
            }
        }
    }

    pub fn compute_pipeline(
        &self,
        sig: &KernelSignature,
    ) -> Result<Arc<wgpu::ComputePipeline>, String> {
        let sig_hash = hash_signature(sig);
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.entry(sig_hash).or_insert_with(|| {
            build_pipeline(&self.device, sig).expect("failed to build pipeline")
        });
        Ok(entry.pipeline.clone())
    }

    pub fn bind_group_layout(
        &self,
        sig: &KernelSignature,
    ) -> Result<Arc<wgpu::BindGroupLayout>, String> {
        let sig_hash = hash_signature(sig);
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.entry(sig_hash).or_insert_with(|| {
            build_pipeline(&self.device, sig).expect("failed to build pipeline")
        });
        Ok(entry.bgls.first().cloned().ok_or_else(|| "no BGL".to_string())?)
    }
}

fn compute_param_size(sig: &KernelSignature) -> u64 {
    sig.params
        .iter()
        .map(|p| match p.kind {
            crate::gpu::kernel::ParameterType::U32 => 4u64,
            crate::gpu::kernel::ParameterType::I32 => 4u64,
            crate::gpu::kernel::ParameterType::F32 => 4u64,
        })
        .sum()
}

fn build_pipeline(
    device: &wgpu::Device,
    sig: &KernelSignature,
) -> Result<CachedPipeline, String> {
    let has_params = !sig.params.is_empty();

    let mut bgls: Vec<Arc<wgpu::BindGroupLayout>> = Vec::new();

    // Group 0: storage buffers
    {
        let mut binding: u32 = 0;
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
        for _ in sig.inputs.iter() {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
            binding += 1;
        }
        for _ in sig.outputs.iter() {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
            binding += 1;
        }
        bgls.push(Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor { label: Some("group0"), entries: &entries },
        )));
    }

    // Group 1: params uniform
    if has_params {
        let bgl = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("group1"),
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
            },
        ));
        bgls.push(bgl);
    }

    let bgl_refs: Vec<&wgpu::BindGroupLayout> = bgls.iter().map(|b| b.as_ref()).collect();

    let spirv = sig.body;
    let mut words: Vec<u32> = vec![0u32; spirv.len() / 4];
    unsafe {
        std::ptr::copy_nonoverlapping(spirv.as_ptr(), words.as_mut_ptr() as *mut u8, spirv.len());
    }
    let source = wgpu::ShaderSource::SpirV(std::borrow::Cow::Owned(words));
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(sig.name),
        source,
    });

    let pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kernel_layout"),
            bind_group_layouts: &bgl_refs,
            push_constant_ranges: &[],
        });

        let pipeline = Arc::new(
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(sig.name),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: sig.entry,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            }),
        );

    Ok(CachedPipeline {
        pipeline,
        bgls,
        has_params,
    })
}

fn build_bind_groups(
    device: &wgpu::Device,
    bgls: &[Arc<wgpu::BindGroupLayout>],
    has_params: bool,
    inputs: &[&GpuBuffer],
    params: &wgpu::Buffer,
    output: &wgpu::Buffer,
) -> Result<(wgpu::BindGroup, Option<wgpu::BindGroup>), String> {
    let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: i as u32,
            resource: input.buffer.as_entire_binding(),
        });
    }
    entries.push(wgpu::BindGroupEntry {
        binding: inputs.len() as u32,
        resource: output.as_entire_binding(),
    });

    let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("group0"),
        layout: &bgls[0],
        entries: &entries,
    });

    let bg1 = if has_params {
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("group1"),
            layout: &bgls[1],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            }],
        }))
    } else {
        None
    };

    Ok((bg0, bg1))
}

fn hash_signature(sig: &KernelSignature) -> u64 {
    let mut h = DefaultHasher::new();
    sig.name.hash(&mut h);
    sig.body.hash(&mut h);
    for i in sig.inputs {
        i.name.hash(&mut h);
    }
    for o in sig.outputs {
        o.name.hash(&mut h);
    }
    h.finish()
}

impl CachedPipeline {
    fn clone_arcs(&self) -> Self {
        Self {
            pipeline: self.pipeline.clone(),
            bgls: self.bgls.clone(),
            has_params: self.has_params,
        }
    }
}
