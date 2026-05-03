/// Mipmap generation via compute shader.
///
/// Handles a single pipeline that downsamples one mip level into the next
/// using a 2×2 box filter.
pub struct MipBuilder {
    pipeline: iced::wgpu::ComputePipeline,
    bgl: iced::wgpu::BindGroupLayout,
    src_view: Option<iced::wgpu::TextureView>,
    dst_view: Option<iced::wgpu::TextureView>,
}

impl MipBuilder {
    pub fn new(device: &iced::wgpu::Device) -> Self {
        let shader = device.create_shader_module(iced::wgpu::ShaderModuleDescriptor {
            label: Some("mip_builder.wgsl"),
            source: iced::wgpu::ShaderSource::Wgsl(MIP_SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&iced::wgpu::BindGroupLayoutDescriptor {
            label: Some("mip_builder_bgl"),
            entries: &[
                iced::wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: iced::wgpu::ShaderStages::COMPUTE,
                    ty: iced::wgpu::BindingType::Texture {
                        sample_type: iced::wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: iced::wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                iced::wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: iced::wgpu::ShaderStages::COMPUTE,
                    ty: iced::wgpu::BindingType::StorageTexture {
                        access: iced::wgpu::StorageTextureAccess::WriteOnly,
                        format: iced::wgpu::TextureFormat::Rgba8UnormSrgb,
                        view_dimension: iced::wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout =
            device.create_pipeline_layout(&iced::wgpu::PipelineLayoutDescriptor {
                label: Some("mip_builder_layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });

        let pipeline =
            device.create_compute_pipeline(&iced::wgpu::ComputePipelineDescriptor {
                label: Some("mip_builder"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("cs_downsample"),
                compilation_options: iced::wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        Self {
            pipeline,
            bgl,
            src_view: None,
            dst_view: None,
        }
    }

    /// Generate one mip level from the source level into the destination level.
    pub fn generate_level(
        &mut self,
        device: &iced::wgpu::Device,
        encoder: &mut iced::wgpu::CommandEncoder,
        texture: &iced::wgpu::Texture,
        src_level: u32,
        dst_level: u32,
    ) {
        let src_view = texture.create_view(&iced::wgpu::TextureViewDescriptor {
            label: Some("mip_src_view"),
            base_mip_level: src_level,
            mip_level_count: Some(1),
            ..Default::default()
        });

        let dst_view = texture.create_view(&iced::wgpu::TextureViewDescriptor {
            label: Some("mip_dst_view"),
            base_mip_level: dst_level,
            mip_level_count: Some(1),
            ..Default::default()
        });

        let bg = device.create_bind_group(&iced::wgpu::BindGroupDescriptor {
            label: Some("mip_bind_group"),
            layout: &self.bgl,
            entries: &[
                iced::wgpu::BindGroupEntry {
                    binding: 0,
                    resource: iced::wgpu::BindingResource::TextureView(&src_view),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 1,
                    resource: iced::wgpu::BindingResource::TextureView(&dst_view),
                },
            ],
        });

        let dst_w = (texture.width() >> dst_level).max(1);
        let dst_h = (texture.height() >> dst_level).max(1);

        let mut pass = encoder.begin_compute_pass(&iced::wgpu::ComputePassDescriptor {
            label: Some("mip_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(dst_w.div_ceil(8), dst_h.div_ceil(8), 1);
    }

    /// Regenerate all MIP levels for the given texture.
    pub fn regenerate_all(
        &mut self,
        device: &iced::wgpu::Device,
        encoder: &mut iced::wgpu::CommandEncoder,
        texture: &iced::wgpu::Texture,
        mip_count: u32,
    ) {
        for level in 0..mip_count - 1 {
            self.generate_level(device, encoder, texture, level, level + 1);
        }
    }
}

const MIP_SHADER: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8, 1)
fn cs_downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(src);
    if (gid.x >= dims.x / 2u || gid.y >= dims.y / 2u) { return; }

    let x = gid.x * 2u;
    let y = gid.y * 2u;
    var c = vec4<f32>(0.0);
    c += textureLoad(src, vec2<i32>(i32(x),     i32(y)),     0);
    c += textureLoad(src, vec2<i32>(i32(x + 1u), i32(y)),     0);
    c += textureLoad(src, vec2<i32>(i32(x),     i32(y + 1u)), 0);
    c += textureLoad(src, vec2<i32>(i32(x + 1u), i32(y + 1u)), 0);
    c *= 0.25;

    textureStore(dst, vec2<i32>(gid.xy), c);
}
"#;
