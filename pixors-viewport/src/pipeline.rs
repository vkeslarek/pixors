//! wgpu resource factories.
//!
//! Centralises all the verbose wgpu descriptor boilerplate so [`viewport`] stays
//! focused on logic rather than GPU object construction.
//!
//! [`viewport`]: crate::viewport

// ── Bind group layout ─────────────────────────────────────────────────────────

/// Creates the shared bind group layout used by both the pipeline and every
/// bind group created in [`update_texture`].
///
/// Bindings (must match `shader.wgsl`):
/// | slot | type              | stage    |
/// |------|-------------------|----------|
/// | 0    | uniform buffer    | fragment |
/// | 1    | 2D texture        | fragment |
/// | 2    | filtering sampler | fragment |
///
/// [`update_texture`]: crate::viewport::PixorsViewport::update_texture
pub(crate) fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("pixors_bgl"),
        entries: &[
            // slot 0 — camera uniform (uv_offset + uv_scale)
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // slot 1 — image texture (RGBA8 sRGB)
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // slot 2 — bilinear sampler
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

// ── Render pipeline ───────────────────────────────────────────────────────────

/// Compiles the WGSL shader and creates the fullscreen-triangle render pipeline.
///
/// The vertex shader generates a clip-space triangle from `vertex_index` — no
/// vertex buffer needed.  The fragment shader samples the image texture through
/// the camera transform and clips pixels outside `[0,1]²` to the background grey.
pub(crate) fn create_render_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pixors_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pixors_pipeline_layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pixors_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[], // positions generated in-shader
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
    })
}

// ── Sampler ───────────────────────────────────────────────────────────────────

/// Creates a bilinear sampler with clamp-to-edge on both axes.
///
/// The shader handles out-of-bounds UVs explicitly, so the address mode only
/// matters for sub-texel sampling at the very edge — clamping prevents seams.
pub(crate) fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("pixors_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}
