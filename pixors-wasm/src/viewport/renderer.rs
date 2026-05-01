use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

use super::camera::CameraUniform;

const ATLAS_BIND_GROUP_LAYOUT_DESC: wgpu::BindGroupLayoutDescriptor = wgpu::BindGroupLayoutDescriptor {
    label: Some("atlas_bind_group_layout"),
    entries: &[
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ],
};

struct Inner {
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
    pending_resize: Option<(u32, u32)>,
}

pub struct Renderer {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    inner: RefCell<Inner>,
    render_pipeline: wgpu::RenderPipeline,
    pub atlas_bind_group_layout: wgpu::BindGroupLayout,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
}

enum MapState {
    Pending(Option<std::task::Waker>),
    Done(Result<(), wgpu::BufferAsyncError>),
}

pub struct MapFuture {
    state: Rc<RefCell<MapState>>,
}

impl std::future::Future for MapFuture {
    type Output = Result<(), wgpu::BufferAsyncError>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut s = self.state.borrow_mut();
        match &*s {
            MapState::Done(_) => {
                if let MapState::Done(r) = std::mem::replace(&mut *s, MapState::Pending(None)) {
                    std::task::Poll::Ready(r)
                } else {
                    unreachable!()
                }
            }
            MapState::Pending(_) => {
                *s = MapState::Pending(Some(cx.waker().clone()));
                std::task::Poll::Pending
            }
        }
    }
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or("no adapter found")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 8192,
                        ..wgpu::Limits::downlevel_defaults()
                    },
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| format!("device: {}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let (texture, buffer, padded_bytes_per_row) =
            Self::create_render_targets(&device, width, height);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tile_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/tile.wgsl").into()),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let atlas_bind_group_layout = device.create_bind_group_layout(&ATLAS_BIND_GROUP_LAYOUT_DESC);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render_pipeline_layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &atlas_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tile_render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<super::atlas::Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, format: wgpu::VertexFormat::Float32x2, shader_location: 0 },
                        wgpu::VertexAttribute { offset: 8, format: wgpu::VertexFormat::Float32x2, shader_location: 1 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
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

        Ok(Self {
            device,
            queue,
            inner: RefCell::new(Inner {
                width,
                height,
                texture,
                buffer,
                padded_bytes_per_row,
                pending_resize: None,
            }),
            render_pipeline,
            atlas_bind_group_layout,
            camera_buffer,
            camera_bind_group,
        })
    }

    fn create_render_targets(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::Buffer, u32) {
        let w = width.max(1);
        let h = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Render Texture"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let padded_bytes_per_row = (w * 4).div_ceil(256) * 256;
        let buffer_size = (padded_bytes_per_row * h) as wgpu::BufferAddress;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Offscreen Copy Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        (texture, buffer, padded_bytes_per_row)
    }

    pub fn queue_resize(&self, width: u32, height: u32) {
        self.inner.borrow_mut().pending_resize = Some((width, height));
    }

    fn apply_resize(&self) {
        let mut inner = self.inner.borrow_mut();
        if let Some((w, h)) = inner.pending_resize.take() {
            if w == 0 || h == 0 || (w == inner.width && h == inner.height) { return; }
            inner.width = w;
            inner.height = h;
            let (t, b, p) = Self::create_render_targets(&self.device, w, h);
            inner.texture = t;
            inner.buffer = b;
            inner.padded_bytes_per_row = p;
        }
    }

    pub fn update_camera(&self, uniform: &CameraUniform) {
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(uniform));
    }

    /// Synchronous: submits the draw commands and starts map_async.
    /// Returns a future that resolves when the buffer is mapped.
    pub fn submit(&self, draw_fn: impl FnOnce(&mut wgpu::RenderPass)) -> MapFuture {
        self.apply_resize();

        let map_state: Rc<RefCell<MapState>> = Rc::new(RefCell::new(MapState::Pending(None)));

        {
            let inner = self.inner.borrow();
            let view = inner.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.165, g: 0.165, b: 0.165, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                rpass.set_pipeline(&self.render_pipeline);
                rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                draw_fn(&mut rpass);
            }

            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &inner.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &inner.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(inner.padded_bytes_per_row),
                        rows_per_image: Some(inner.height),
                    },
                },
                wgpu::Extent3d { width: inner.width.max(1), height: inner.height.max(1), depth_or_array_layers: 1 },
            );

            self.queue.submit(std::iter::once(encoder.finish()));

            let state_cb = map_state.clone();
            inner.buffer.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                let prev = std::mem::replace(&mut *state_cb.borrow_mut(), MapState::Done(res));
                if let MapState::Pending(Some(w)) = prev { w.wake(); }
            });
        }

        MapFuture { state: map_state }
    }

    /// Async: reads the mapped buffer and returns RGBA8 pixels as Uint8Array.
    pub fn read_pixels(&self) -> Result<js_sys::Uint8Array, JsValue> {
        let inner = self.inner.borrow();
        let data = inner.buffer.slice(..).get_mapped_range();
        let unpadded_bytes_per_row = (inner.width * 4) as usize;
        let total = unpadded_bytes_per_row * inner.height as usize;
        let mut out = Vec::with_capacity(total);
        for row in 0..inner.height as usize {
            let start = row * inner.padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row;
            out.extend_from_slice(&data[start..end]);
        }
        drop(data);
        inner.buffer.unmap();
        let js_array = js_sys::Uint8Array::new_with_length(out.len() as u32);
        js_array.copy_from(&out);
        Ok(js_array)
    }
}
