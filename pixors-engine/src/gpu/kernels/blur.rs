use std::sync::{Arc, OnceLock};

use crate::gpu::context::GpuContext;

/// WGSL box-blur compute shader. Operates on a packed `vec4<u32>` buffer
/// where each `u32` holds RGBA8 (one pixel). Layout: `[width, height,
/// radius, _pad]` then row-major pixels.
const SHADER_SRC: &str = r#"
struct Params {
    width: u32,
    height: u32,
    radius: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;

fn unpack(p: u32) -> vec4<u32> {
    return vec4<u32>(p & 0xffu, (p >> 8u) & 0xffu, (p >> 16u) & 0xffu, (p >> 24u) & 0xffu);
}
fn pack(v: vec4<u32>) -> u32 {
    return (v.x & 0xffu) | ((v.y & 0xffu) << 8u) | ((v.z & 0xffu) << 16u) | ((v.w & 0xffu) << 24u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.width;
    let h = params.height;
    let r = i32(params.radius);
    if (gid.x >= w || gid.y >= h) { return; }
    var sum: vec4<u32> = vec4<u32>(0u);
    var count: u32 = 0u;
    let cx = i32(gid.x);
    let cy = i32(gid.y);
    for (var dy: i32 = -r; dy <= r; dy = dy + 1) {
        let yy = clamp(cy + dy, 0, i32(h) - 1);
        for (var dx: i32 = -r; dx <= r; dx = dx + 1) {
            let xx = clamp(cx + dx, 0, i32(w) - 1);
            let off = u32(yy) * w + u32(xx);
            sum = sum + unpack(src[off]);
            count = count + 1u;
        }
    }
    let avg = sum / vec4<u32>(count);
    dst[gid.y * w + gid.x] = pack(avg);
}
"#;

pub struct BlurPipeline {
    pub pipeline: wgpu::ComputePipeline,
    pub bgl: wgpu::BindGroupLayout,
}

static PIPELINE: OnceLock<Arc<BlurPipeline>> = OnceLock::new();

pub fn get_or_init(ctx: &GpuContext) -> Arc<BlurPipeline> {
    PIPELINE
        .get_or_init(|| {
            let shader = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("blur-shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
            });
            let bgl = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blur-bgl"),
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
            });
            let layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blur-layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });
            let pipeline = ctx.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("blur-pipeline"),
                layout: Some(&layout),
                module: &shader,
                entry_point: "main",
                cache: None,
                compilation_options: Default::default(),
            });
            Arc::new(BlurPipeline { pipeline, bgl })
        })
        .clone()
}
