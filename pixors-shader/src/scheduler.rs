use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::kernel::{GpuKernel, KernelSig};
use crate::pool::BufferPool;

const BATCH_SIZE: usize = 16;

struct CachedPipeline {
    pipeline: Arc<wgpu::ComputePipeline>,
    bgl: Arc<wgpu::BindGroupLayout>,
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
        let sig = kernel.sig();
        let sig_hash = hash_sig(sig);

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

        let bind_group = build_bind_group(
            &self.device,
            &cached.bgl,
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
            pass.set_bind_group(0, &bind_group, &[]);
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

    pub fn compute_pipeline(
        &self,
        sig: &KernelSig,
    ) -> Result<Arc<wgpu::ComputePipeline>, String> {
        let sig_hash = hash_sig(sig);
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.entry(sig_hash).or_insert_with(|| {
            build_pipeline(&self.device, sig).expect("failed to build pipeline")
        });
        Ok(entry.pipeline.clone())
    }

    pub fn bind_group_layout(
        &self,
        sig: &KernelSig,
    ) -> Result<Arc<wgpu::BindGroupLayout>, String> {
        let sig_hash = hash_sig(sig);
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.entry(sig_hash).or_insert_with(|| {
            build_pipeline(&self.device, sig).expect("failed to build pipeline")
        });
        Ok(entry.bgl.clone())
    }
}

fn compute_param_size(sig: &KernelSig) -> u64 {
    sig.params
        .iter()
        .map(|p| match p.ty {
            crate::kernel::ParamType::U32 => 4u64,
            crate::kernel::ParamType::I32 => 4u64,
            crate::kernel::ParamType::F32 => 4u64,
        })
        .sum()
}

fn build_pipeline(
    device: &wgpu::Device,
    sig: &KernelSig,
) -> Result<CachedPipeline, String> {
    let has_params = !sig.params.is_empty();
    let num_inputs = sig.inputs.len() as u32;
    let num_outputs = sig.outputs.len() as u32;

    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
    let mut binding: u32 = 0;

    if has_params {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        binding += 1;
    }

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

    let _ = num_inputs + num_outputs;

    let bgl = Arc::new(
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(sig.name),
            entries: &entries,
        }),
    );

    let wgsl = assemble_wgsl(sig);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(sig.name),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });

    let pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kernel_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

    let pipeline = Arc::new(
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(sig.name),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "entry",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        }),
    );

    Ok(CachedPipeline {
        pipeline,
        bgl,
        has_params,
    })
}

fn build_bind_group(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    has_params: bool,
    inputs: &[&GpuBuffer],
    params: &wgpu::Buffer,
    output: &wgpu::Buffer,
) -> Result<wgpu::BindGroup, String> {
    let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
    let mut binding: u32 = 0;

    if has_params {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: params.as_entire_binding(),
        });
        binding += 1;
    }

    for input in inputs {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: input.buffer.as_entire_binding(),
        });
        binding += 1;
    }

    entries.push(wgpu::BindGroupEntry {
        binding,
        resource: output.as_entire_binding(),
    });

    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kernel_bind_group"),
        layout: bgl,
        entries: &entries,
    }))
}

fn hash_sig(sig: &KernelSig) -> u64 {
    let mut h = DefaultHasher::new();
    sig.name.hash(&mut h);
    sig.body_wgsl.hash(&mut h);
    for i in sig.inputs {
        i.name.hash(&mut h);
    }
    for o in sig.outputs {
        o.name.hash(&mut h);
    }
    h.finish()
}

fn assemble_wgsl(sig: &KernelSig) -> String {
    let mut wgsl = String::new();

    if !sig.params.is_empty() {
        wgsl.push_str("struct Params {\n");
        for p in sig.params {
            let ty = match p.ty {
                crate::kernel::ParamType::U32 => "u32",
                crate::kernel::ParamType::I32 => "i32",
                crate::kernel::ParamType::F32 => "f32",
            };
            wgsl.push_str(&format!("    {}: {},\n", p.name, ty));
        }
        wgsl.push_str("};\n");
        wgsl.push_str("@group(0) @binding(0) var<uniform> params: Params;\n");
    }

    for (i, inp) in sig.inputs.iter().enumerate() {
        let b = if sig.params.is_empty() { i as u32 } else { i as u32 + 1 };
        wgsl.push_str(&format!(
            "@group(0) @binding({}) var<storage, read> {}: array<u32>;\n",
            b, inp.name
        ));
    }

    for (i, out) in sig.outputs.iter().enumerate() {
        let b = sig.inputs.len() as u32
            + if sig.params.is_empty() { i as u32 } else { i as u32 + 1 };
        wgsl.push_str(&format!(
            "@group(0) @binding({}) var<storage, read_write> {}: array<u32>;\n",
            b, out.name
        ));
    }

    wgsl.push_str(sig.body_wgsl);
    wgsl.push('\n');
    wgsl
}

impl CachedPipeline {
    fn clone_arcs(&self) -> Self {
        Self {
            pipeline: self.pipeline.clone(),
            bgl: self.bgl.clone(),
            has_params: self.has_params,
        }
    }
}
