use std::sync::{Arc, Mutex};

use iced::widget::shader::{self, Viewport};
use iced::{Event, Point, Rectangle, Size};
use iced::mouse;

use pixors_engine::pipeline::exec::display_sink::GpuBufferState;

use crate::viewport::camera::{Camera, CameraUniform};

// wgpu is re-exported by iced; use the crate directly for texture upload helpers.

pub struct EngineProgram {
    pub image: Arc<Mutex<GpuBufferState>>,
}

impl<Msg> shader::Program<Msg> for EngineProgram {
    type State = EngineState;
    type Primitive = EnginePrimitive;

    fn draw(
        &self,
        state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        EnginePrimitive {
            camera: state.camera.to_uniform(),
            image: self.image.clone(),
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        let size = Size::new(bounds.width, bounds.height);
        if state.last_bounds.map_or(true, |s| s != size) {
            state.camera.resize(size.width, size.height);
            if !state.fitted {
                state.camera.fit();
                state.fitted = true;
            }
            state.last_bounds = Some(size);
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.position_in(bounds).is_some() {
                    state.dragging = true;
                    state.last_pos = cursor.position_in(bounds);
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                state.last_pos = None;
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(curr) = cursor.position_in(bounds) {
                        if let Some(last) = state.last_pos {
                            let dx = curr.x - last.x;
                            let dy = curr.y - last.y;
                            state.camera.pan(dx, dy);
                        }
                        state.last_pos = Some(curr);
                    }
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_some() {
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y * 24.0,
                        mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    let factor = if dy > 0.0 {
                        1.1_f32.powf(dy)
                    } else {
                        1.0 / 1.1_f32.powf(-dy)
                    };
                    let pos = cursor
                        .position_in(bounds)
                        .unwrap_or(Point::new(0.0, 0.0));
                    state.camera.zoom_at(factor, pos.x, pos.y);
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::default()
        }
    }
}

pub struct EngineState {
    camera: Camera,
    dragging: bool,
    fitted: bool,
    last_pos: Option<Point>,
    last_bounds: Option<Size>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            camera: Camera::new(2048.0, 1536.0),
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
        }
    }
}

#[derive(Clone)]
pub struct EnginePrimitive {
    camera: CameraUniform,
    image: Arc<Mutex<GpuBufferState>>,
}

impl std::fmt::Debug for EnginePrimitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnginePrimitive").field("camera", &self.camera).finish()
    }
}

pub struct EnginePipeline {
    pipeline: iced::wgpu::RenderPipeline,
    camera_buffer: iced::wgpu::Buffer,
    bind_group: iced::wgpu::BindGroup,
    texture: Option<TextureResources>,
    texture_dims: Option<(u32, u32)>,
    format: iced::wgpu::TextureFormat,
}

struct TextureResources {
    tex: iced::wgpu::Texture,
    view: iced::wgpu::TextureView,
    sampler: iced::wgpu::Sampler,
}

impl EnginePipeline {
    fn create_texture(
        &mut self,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        pixels: &[u8],
        w: u32,
        h: u32,
    ) {
        let extent = iced::wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };

        let tex = device.create_texture(&iced::wgpu::TextureDescriptor {
            label: Some("image_texture"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: iced::wgpu::TextureDimension::D2,
            format: iced::wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: iced::wgpu::TextureUsages::TEXTURE_BINDING
                | iced::wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let pad = (256 - (w * 4) % 256) % 256;
        let row_pitch = w * 4 + pad;
        let mut padded = vec![0u8; (row_pitch * h) as usize];
        for y in 0..h as usize {
            let src = y * w as usize * 4;
            let dst = y * row_pitch as usize;
            padded[dst..dst + w as usize * 4].copy_from_slice(&pixels[src..src + w as usize * 4]);
        }

        let staging = device.create_buffer(&iced::wgpu::BufferDescriptor {
            label: Some("image_staging"),
            size: padded.len() as u64,
            usage: iced::wgpu::BufferUsages::COPY_SRC | iced::wgpu::BufferUsages::MAP_WRITE,
            mapped_at_creation: true,
        });
        staging.slice(..).get_mapped_range_mut()[..padded.len()].copy_from_slice(&padded);
        staging.unmap();

        let mut encoder = device.create_command_encoder(&iced::wgpu::CommandEncoderDescriptor {
            label: Some("image_upload"),
        });
        encoder.copy_buffer_to_texture(
            iced::wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: iced::wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_pitch),
                    rows_per_image: Some(h),
                },
            },
            iced::wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: iced::wgpu::Origin3d::ZERO,
                aspect: iced::wgpu::TextureAspect::All,
            },
            extent,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let view = tex.create_view(&iced::wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&iced::wgpu::SamplerDescriptor {
            address_mode_u: iced::wgpu::AddressMode::ClampToEdge,
            address_mode_v: iced::wgpu::AddressMode::ClampToEdge,
            address_mode_w: iced::wgpu::AddressMode::ClampToEdge,
            mag_filter: iced::wgpu::FilterMode::Linear,
            min_filter: iced::wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device
            .create_bind_group_layout(&iced::wgpu::BindGroupLayoutDescriptor {
                label: Some("engine_bind_group_layout"),
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

        let bind_group = device.create_bind_group(&iced::wgpu::BindGroupDescriptor {
            label: Some("engine_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                iced::wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buffer.as_entire_binding(),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 1,
                    resource: iced::wgpu::BindingResource::TextureView(&view),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 2,
                    resource: iced::wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device
            .create_pipeline_layout(&iced::wgpu::PipelineLayoutDescriptor {
                label: Some("engine_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let shader = device.create_shader_module(iced::wgpu::ShaderModuleDescriptor {
            label: Some("engine.wgsl"),
            source: iced::wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let render_pipeline = device
            .create_render_pipeline(&iced::wgpu::RenderPipelineDescriptor {
                label: Some("engine"),
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
                        format: self.format,
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

        self.texture = Some(TextureResources { tex, view, sampler });
        self.texture_dims = Some((w, h));
        self.bind_group = bind_group;
        self.pipeline = render_pipeline;
    }
}

impl shader::Pipeline for EnginePipeline {
    fn new(
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        format: iced::wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(iced::wgpu::ShaderModuleDescriptor {
            label: Some("engine.wgsl"),
            source: iced::wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // Placeholder 1x1 texture so the pipeline layout has bindings 0,1,2 from start
        let dummy_extent = iced::wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };
        let dummy_tex = device.create_texture(&iced::wgpu::TextureDescriptor {
            label: Some("dummy_texture"),
            size: dummy_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: iced::wgpu::TextureDimension::D2,
            format: iced::wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: iced::wgpu::TextureUsages::TEXTURE_BINDING
                | iced::wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_data = [0u8; 4];
        queue.write_texture(
            iced::wgpu::TexelCopyTextureInfo {
                texture: &dummy_tex,
                mip_level: 0,
                origin: iced::wgpu::Origin3d::ZERO,
                aspect: iced::wgpu::TextureAspect::All,
            },
            &dummy_data,
            iced::wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            dummy_extent,
        );
        let dummy_view = dummy_tex.create_view(&iced::wgpu::TextureViewDescriptor::default());
        let dummy_sampler = device.create_sampler(&iced::wgpu::SamplerDescriptor {
            address_mode_u: iced::wgpu::AddressMode::ClampToEdge,
            address_mode_v: iced::wgpu::AddressMode::ClampToEdge,
            address_mode_w: iced::wgpu::AddressMode::ClampToEdge,
            mag_filter: iced::wgpu::FilterMode::Linear,
            min_filter: iced::wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout =
            device.create_bind_group_layout(&iced::wgpu::BindGroupLayoutDescriptor {
                label: Some("engine_bind_group_layout"),
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

        let pipeline_layout = device
            .create_pipeline_layout(&iced::wgpu::PipelineLayoutDescriptor {
                label: Some("engine_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = device
            .create_render_pipeline(&iced::wgpu::RenderPipelineDescriptor {
                label: Some("engine"),
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

        let bind_group = device.create_bind_group(&iced::wgpu::BindGroupDescriptor {
            label: Some("engine_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                iced::wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 1,
                    resource: iced::wgpu::BindingResource::TextureView(&dummy_view),
                },
                iced::wgpu::BindGroupEntry {
                    binding: 2,
                    resource: iced::wgpu::BindingResource::Sampler(&dummy_sampler),
                },
            ],
        });

        Self {
            pipeline,
            camera_buffer,
            bind_group,
            texture: Some(TextureResources {
                tex: dummy_tex,
                view: dummy_view,
                sampler: dummy_sampler,
            }),
            texture_dims: Some((1, 1)),
            format,
        }
    }
}

impl shader::Primitive for EnginePrimitive {
    type Pipeline = EnginePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        queue.write_buffer(
            &pipeline.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera),
        );

        let guard = match self.image.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let img_w = guard.width;
        let img_h = guard.height;

        if img_w == 0 || img_h == 0 {
            return;
        }

        let dims_changed = pipeline.texture_dims != Some((img_w, img_h));
        if guard.dirty || dims_changed {
            pipeline.create_texture(device, queue, &guard.pixels, img_w, img_h);
        }
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut iced::wgpu::CommandEncoder,
        target: &iced::wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&iced::wgpu::RenderPassDescriptor {
            label: Some("engine pass"),
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

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let ix = pos.x / cam.zoom + cam.pan_x;
    let iy = pos.y / cam.zoom + cam.pan_y;

    if ix < 0.0 || iy < 0.0 || ix >= cam.img_w || iy >= cam.img_h {
        return vec4<f32>(0.067, 0.067, 0.075, 1.0);
    }

    let u = ix / cam.img_w;
    let v = 1.0 - iy / cam.img_h;
    return textureSample(t, s, vec2<f32>(u, v));
}
"#;
