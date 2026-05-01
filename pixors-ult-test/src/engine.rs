use iced::event;
use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::{Point, Rectangle, Size};

pub use iced::widget::shader::wgpu;

use crate::viewport::camera::{Camera, CameraUniform};

pub struct EngineProgram;

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
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: shader::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        _shell: &mut iced::advanced::Shell<'_, Msg>,
    ) -> (event::Status, Option<Msg>) {
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
            shader::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.position_in(bounds).is_some() {
                    state.dragging = true;
                    state.last_pos = cursor.position_in(bounds);
                    (event::Status::Captured, None)
                } else {
                    (event::Status::Ignored, None)
                }
            }
            shader::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                state.last_pos = None;
                (event::Status::Ignored, None)
            }
            shader::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(curr) = cursor.position_in(bounds) {
                        if let Some(last) = state.last_pos {
                            let dx = curr.x - last.x;
                            let dy = curr.y - last.y;
                            state.camera.pan(dx, dy);
                        }
                        state.last_pos = Some(curr);
                    }
                    (event::Status::Captured, None)
                } else {
                    (event::Status::Ignored, None)
                }
            }
            shader::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_some() {
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y * 24.0,
                        mouse::ScrollDelta::Pixels { y, .. } => y,
                    };
                    let factor = if dy > 0.0 { 1.1_f32.powf(dy) } else { 1.0 / 1.1_f32.powf(-dy) };
                    let pos = cursor.position_in(bounds).unwrap_or(Point::new(0.0, 0.0));
                    state.camera.zoom_at(factor, pos.x, pos.y);
                    (event::Status::Captured, None)
                } else {
                    (event::Status::Ignored, None)
                }
            }
            _ => (event::Status::Ignored, None),
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

#[derive(Debug)]
pub struct EnginePrimitive {
    camera: CameraUniform,
}

struct EnginePipeline {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
}

impl EnginePipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("engine.wgsl"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("engine_pipeline_layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("engine"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
        }
    }
}

impl shader::Primitive for EnginePrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if !storage.has::<EnginePipeline>() {
            storage.store(EnginePipeline::new(device, format));
        }
        let pipeline = storage.get::<EnginePipeline>().unwrap();
        queue.write_buffer(
            &pipeline.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera),
        );
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<EnginePipeline>().unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("engine pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
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
        pass.set_bind_group(0, &pipeline.camera_bind_group, &[]);
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

    let cell = 64.0;
    let xi = u32(floor(ix / cell));
    let yi = u32(floor(iy / cell));
    let v  = f32((xi + yi) & 1u);
    let dark  = vec3<f32>(0.157, 0.157, 0.176);
    let light = vec3<f32>(0.863, 0.863, 0.863);
    return vec4<f32>(mix(dark, light, v), 1.0);
}
"#;
