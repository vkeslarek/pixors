use std::sync::{Arc, Mutex};

use iced::widget::shader;

use crate::viewport::camera::CameraUniform;
use crate::viewport::mip_builder::MipBuilder;
use crate::viewport::tiled_texture::TiledTexture;

pub struct ViewportPipeline {
    pub pipeline: iced::wgpu::RenderPipeline,
    pub camera_buffer: iced::wgpu::Buffer,
    pub bind_group: iced::wgpu::BindGroup,
    pub bgl: iced::wgpu::BindGroupLayout,
    pub format: iced::wgpu::TextureFormat,
    pub tiled_texture: Option<Arc<Mutex<TiledTexture>>>,
    pub mip_builder: MipBuilder,
    texture_dims: Option<(u32, u32)>,
}

impl shader::Pipeline for ViewportPipeline {
    fn new(
        device: &iced::wgpu::Device,
        _queue: &iced::wgpu::Queue,
        format: iced::wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(iced::wgpu::ShaderModuleDescriptor {
            label: Some("viewport.wgsl"),
            source: iced::wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&iced::wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport_bgl"),
            entries: &[
                iced::wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: iced::wgpu::ShaderStages::FRAGMENT,
                    ty: iced::wgpu::BindingType::Buffer {
                        ty: iced::wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                iced::wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: iced::wgpu::ShaderStages::FRAGMENT,
                    ty: iced::wgpu::BindingType::Texture {
                        sample_type: iced::wgpu::TextureSampleType::Float {
                            filterable: true,
                        },
                        view_dimension: iced::wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                iced::wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: iced::wgpu::ShaderStages::FRAGMENT,
                    ty: iced::wgpu::BindingType::Sampler(
                        iced::wgpu::SamplerBindingType::Filtering,
                    ),
                    count: None,
                },
            ],
        });

        let pipeline_layout =
            device.create_pipeline_layout(&iced::wgpu::PipelineLayoutDescriptor {
                label: Some("viewport_layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });

        let pipeline =
            device.create_render_pipeline(&iced::wgpu::RenderPipelineDescriptor {
                label: Some("viewport"),
                layout: Some(&pipeline_layout),
                vertex: iced::wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: iced::wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(iced::wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(iced::wgpu::ColorTargetState {
                        format,
                        blend: Some(iced::wgpu::BlendState::REPLACE),
                        write_mask: iced::wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: iced::wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: iced::wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: iced::wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let camera_buffer = device.create_buffer(&iced::wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: iced::wgpu::BufferUsages::UNIFORM | iced::wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Placeholder 1×1 texture for initial bind group
        let dummy_tex = device.create_texture(&iced::wgpu::TextureDescriptor {
            label: Some("dummy"),
            size: iced::wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: iced::wgpu::TextureDimension::D2,
            format: iced::wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: iced::wgpu::TextureUsages::TEXTURE_BINDING
                | iced::wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_view = dummy_tex.create_view(&iced::wgpu::TextureViewDescriptor::default());
        let dummy_sampler = device.create_sampler(&iced::wgpu::SamplerDescriptor::default());

        let bind_group = ViewportPipeline::build_bind_group(
            device,
            &bgl,
            &camera_buffer,
            &dummy_view,
            &dummy_sampler,
        );

        Self {
            pipeline,
            camera_buffer,
            bind_group,
            bgl,
            format,
            tiled_texture: None,
            mip_builder: MipBuilder::new(device),
            texture_dims: None,
        }
    }
}

impl ViewportPipeline {
    fn build_bind_group(
        device: &iced::wgpu::Device,
        layout: &iced::wgpu::BindGroupLayout,
        camera: &iced::wgpu::Buffer,
        tex_view: &iced::wgpu::TextureView,
        sampler: &iced::wgpu::Sampler,
    ) -> iced::wgpu::BindGroup {
        device.create_bind_group(&iced::wgpu::BindGroupDescriptor {
            label: Some("viewport_bg"),
            layout,
            entries: &[
                iced::wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 1,
                    resource: iced::wgpu::BindingResource::TextureView(tex_view),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 2,
                    resource: iced::wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn update_camera(&self, queue: &iced::wgpu::Queue, camera: &CameraUniform) {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(camera));
    }

    pub fn maybe_rebind(
        &mut self,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
    ) -> bool {
        let Some(tex) = &self.tiled_texture else {
            return false;
        };
        let guard = tex.lock().unwrap();
        let dims = guard.dims();
        let dims_changed = self.texture_dims != Some(dims);
        let needs_mip = guard.mip_dirty;
        drop(guard);

        if dims_changed || needs_mip {
            if needs_mip {
                let mut encoder =
                    device.create_command_encoder(&iced::wgpu::CommandEncoderDescriptor {
                        label: Some("mip_regenerate"),
                    });
                let mut guard = tex.lock().unwrap();
                self.mip_builder.regenerate_all(
                    device,
                    &mut encoder,
                    &guard.texture,
                    guard.mip_count,
                );
                guard.mip_dirty = false;
                guard.dirty_tiles.clear();
                queue.submit(std::iter::once(encoder.finish()));
            }

            let guard = tex.lock().unwrap();
            self.bind_group = Self::build_bind_group(
                device,
                &self.bgl,
                &self.camera_buffer,
                guard.view(),
                guard.sampler(),
            );
            self.texture_dims = Some(dims);
            return true;
        }
        false
    }
}

#[derive(Debug)]
pub struct ViewportPrimitive {
    pub camera: CameraUniform,
}

impl shader::Primitive for ViewportPrimitive {
    type Pipeline = ViewportPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        _bounds: &iced::Rectangle,
        _viewport: &iced::widget::shader::Viewport,
    ) {
        pipeline.update_camera(queue, &self.camera);
        pipeline.maybe_rebind(device, queue);
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut iced::wgpu::CommandEncoder,
        target: &iced::wgpu::TextureView,
        clip_bounds: &iced::Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&iced::wgpu::RenderPassDescriptor {
            label: Some("viewport pass"),
            color_attachments: &[Some(iced::wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: iced::wgpu::Operations {
                    load: iced::wgpu::LoadOp::Load,
                    store: iced::wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

const SHADER: &str = r#"
struct Camera {
    vp_w:  f32,
    vp_h:  f32,
    img_w: f32,
    img_h: f32,
    pan_x: f32,
    pan_y: f32,
    zoom:  f32,
    _pad:  f32,
}
@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var t: texture_2d<f32>;
@group(0) @binding(2) var s: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    var o: VsOut;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    o.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return o;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let screen = in.uv * vec2<f32>(cam.vp_w, cam.vp_h);
    let img_xy = screen / cam.zoom + vec2<f32>(cam.pan_x, cam.pan_y);
    if (img_xy.x < 0.0 || img_xy.y < 0.0
        || img_xy.x >= cam.img_w || img_xy.y >= cam.img_h) {
        return vec4<f32>(0.067, 0.067, 0.075, 1.0);
    }
    let uv = img_xy / vec2<f32>(cam.img_w, cam.img_h);
    return textureSample(t, s, uv);
}
"#;
