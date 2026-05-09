use crate::gpu::kernel::KernelSignature;
use dashmap::DashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

/// Compiled compute pipeline cached by signature hash.
/// Build once per unique kernel, reused across dispatches.
pub struct CachedPipeline {
    pub pipeline: Arc<wgpu::ComputePipeline>,
    pub bgls: Vec<Arc<wgpu::BindGroupLayout>>,
    pub has_params: bool,
}

impl CachedPipeline {
    pub fn clone_arcs(&self) -> Self {
        Self {
            pipeline: self.pipeline.clone(),
            bgls: self.bgls.clone(),
            has_params: self.has_params,
        }
    }
}

/// Lock-free pipeline cache keyed by signature hash.
pub struct PipelineCache {
    map: DashMap<u64, CachedPipeline>,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn get_or_build(
        &self,
        sig_hash: u64,
        sig: &KernelSignature,
        device: &wgpu::Device,
    ) -> CachedPipeline {
        if let Some(cached) = self.map.get(&sig_hash) {
            return cached.clone_arcs();
        }
        let pipeline = build_pipeline(device, sig).expect("failed to build pipeline");
        let cloned = pipeline.clone_arcs();
        self.map.insert(sig_hash, pipeline);
        cloned
    }
}

/// Hash a kernel signature for cache lookup.
pub fn hash_signature(sig: &KernelSignature) -> u64 {
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

/// Total byte size of kernel params (u32/i32/f32 = 4 bytes each).
pub fn compute_param_size(sig: &KernelSignature) -> u64 {
    sig.params
        .iter()
        .map(|p| match p.kind {
            crate::gpu::kernel::ParameterType::U32 => 4u64,
            crate::gpu::kernel::ParameterType::I32 => 4u64,
            crate::gpu::kernel::ParameterType::F32 => 4u64,
        })
        .sum()
}

/// Create a fresh uniform buffer, upload kernel params into it.
pub fn upload_params(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sig: &KernelSignature,
    kernel: &dyn crate::gpu::kernel::GpuKernel,
) -> wgpu::Buffer {
    let size = compute_param_size(sig).max(16);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("params"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut data = vec![0u8; size as usize];
    kernel.write_params(&mut data);
    queue.write_buffer(&buf, 0, &data);
    buf
}

/// Build bind groups for a kernel dispatch (group 0 = storage buffers, optional group 1 = params).
pub fn build_bind_groups(
    device: &wgpu::Device,
    cached: &CachedPipeline,
    inputs: &[&wgpu::Buffer],
    params: &wgpu::Buffer,
    output: &wgpu::Buffer,
) -> (wgpu::BindGroup, Option<wgpu::BindGroup>) {
    let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
    for (i, buf) in inputs.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: i as u32,
            resource: buf.as_entire_binding(),
        });
    }
    entries.push(wgpu::BindGroupEntry {
        binding: inputs.len() as u32,
        resource: output.as_entire_binding(),
    });

    let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("group0"),
        layout: &cached.bgls[0],
        entries: &entries,
    });

    let bg1 = if cached.has_params {
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("group1"),
            layout: &cached.bgls[1],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            }],
        }))
    } else {
        None
    };

    (bg0, bg1)
}

// ── Pipeline construction ──────────────────────────────────────────────────

fn build_pipeline(device: &wgpu::Device, sig: &KernelSignature) -> Result<CachedPipeline, String> {
    let has_params = !sig.params.is_empty();
    let mut bgls: Vec<Arc<wgpu::BindGroupLayout>> = Vec::new();

    // Group 0: storage buffers (inputs + output)
    {
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
        for i in 0..(sig.inputs.len() + sig.outputs.len()) {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: i as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }
        bgls.push(Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("group0"),
                entries: &entries,
            },
        )));
    }

    // Group 1: params uniform
    if has_params {
        bgls.push(Arc::new(device.create_bind_group_layout(
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
        )));
    }

    let bgl_refs: Vec<&wgpu::BindGroupLayout> = bgls.iter().map(|b| b.as_ref()).collect();
    let spirv = sig.body;
    let mut words: Vec<u32> = vec![0u32; spirv.len() / 4];
    unsafe {
        std::ptr::copy_nonoverlapping(spirv.as_ptr(), words.as_mut_ptr() as *mut u8, spirv.len());
    }
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(sig.name),
        source: wgpu::ShaderSource::SpirV(std::borrow::Cow::Owned(words)),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
