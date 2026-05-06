use std::sync::{Arc, Mutex};

use iced::widget::shader;

use crate::viewport::camera::CameraUniform;
use crate::viewport::tile_cache::ViewportCache;
use crate::viewport::tiled_texture::TiledTexture;

pub struct ViewportPipeline {
    pipeline: iced::wgpu::RenderPipeline,
    camera_buffer: iced::wgpu::Buffer,
    bind_group: iced::wgpu::BindGroup,
    bgl: iced::wgpu::BindGroupLayout,
    tiled_texture: Option<Arc<Mutex<TiledTexture>>>,
    texture_dims: Option<(u32, u32)>,
    last_mip: Option<u32>,
    use_linear: bool,
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
        let dummy_view =
            dummy_tex.create_view(&iced::wgpu::TextureViewDescriptor::default());
        let dummy_sampler =
            device.create_sampler(&iced::wgpu::SamplerDescriptor::default());

        let bind_group = Self::make_bind_group(
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
            tiled_texture: None,
            texture_dims: None,
            last_mip: None,
            use_linear: true,
        }
    }
}

impl ViewportPipeline {
    fn make_bind_group(
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

    fn rebind_if_needed(&mut self, device: &iced::wgpu::Device, use_linear: bool) {
        let Some(tex) = &self.tiled_texture else {
            return;
        };
        let guard = tex.lock().unwrap();
        let dims = guard.dims();
        if self.texture_dims == Some(dims) && self.use_linear == use_linear {
            return;
        }
        self.bind_group = Self::make_bind_group(
            device,
            &self.bgl,
            &self.camera_buffer,
            guard.view(),
            guard.sampler(use_linear),
        );
        self.texture_dims = Some(dims);
        self.use_linear = use_linear;
    }
}

pub struct ViewportPrimitive {
    pub(super) camera: CameraUniform,
    pub(super) cache: Option<Arc<Mutex<ViewportCache>>>,
    pub(super) visible_range: pixors_executor::source::cache_reader::TileRange,
}

impl Clone for ViewportPrimitive {
    fn clone(&self) -> Self {
        Self {
            camera: self.camera.clone(),
            cache: self.cache.clone(),
            visible_range: pixors_executor::source::cache_reader::TileRange { tx_start: 0, tx_end: 0, ty_start: 0, ty_end: 0 },
        }
    }
}

impl std::fmt::Debug for ViewportPrimitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPrimitive")
            .field("camera", &self.camera)
            .finish_non_exhaustive()
    }
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
        let mip = self.camera.mip_level as u32;
        let tex_w = self.camera.img_w as u32;
        let tex_h = self.camera.img_h as u32;

        let Some(ref cache_arc) = self.cache else { return; };
        let Ok(mut cache) = cache_arc.lock() else { return; };

        cache.set_active_mip(mip);

        let img_w0 = self.camera.img_w0 as u32;
        let img_h0 = self.camera.img_h0 as u32;

        // If the primitive was created before the cache's image dimensions were updated,
        // it is stale. Abort preparation so we don't prematurely drain pending tiles into
        // an incorrectly sized texture, which would cause a wgpu out-of-bounds panic.
        if (img_w0, img_h0) != cache.active_dims {
            return;
        }

        // Full re-upload when MIP level changes or texture was resized (new image).
        let full_reload = pipeline.last_mip != Some(mip)
            || pipeline.texture_dims != Some((tex_w, tex_h));

        ensure_texture(&mut pipeline.tiled_texture, device, queue, tex_w, tex_h, mip);

        if let Some(ref tex_arc) = pipeline.tiled_texture {
            let tex = tex_arc.lock().unwrap();
            let mut pending = cache.take_pending_keys_for_mip(mip);
            
            let tiles: Vec<_> = if full_reload {
                cache.tiles_in_range(mip, &self.visible_range)
            } else {
                pending.drain(..).filter_map(|k| cache.get(&k).map(|v| (k, v))).collect()
            };

            for (_, tile) in tiles {
                tex.write_tile_cpu(
                    queue,
                    tile.px,
                    tile.py,
                    tile.width,
                    tile.height,
                    &tile.bytes,
                );
            }
        }

        let use_linear = self.camera.zoom < 4.0;
        pipeline.last_mip = Some(mip);
        pipeline.rebind_if_needed(device, use_linear);
        queue.write_buffer(
            &pipeline.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera),
        );
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
        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn ensure_texture(
    tex: &mut Option<Arc<Mutex<TiledTexture>>>,
    device: &iced::wgpu::Device,
    queue: &iced::wgpu::Queue,
    width: u32,
    height: u32,
    mip: u32,
) {
    if let Some(arc) = tex {
        if let Ok(mut guard) = arc.lock() {
            guard.resize(device, queue, width, height, mip);
        }
    } else {
        *tex = Some(Arc::new(Mutex::new(TiledTexture::new(
            device, queue, width, height, 256, mip,
        ))));
    }
}

const SHADER: &str = r#"
struct Camera {
    vp_w: f32, vp_h: f32,
    img_w: f32, img_h: f32,
    pan_x: f32, pan_y: f32,
    zoom: f32, mip_level: f32,
    img_w0: f32, img_h0: f32,
    _pad0: f32, _pad1: f32,
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
    let img_xy_mip0 = screen / cam.zoom + vec2<f32>(cam.pan_x, cam.pan_y);

    // Use exact ratio (mip0 dims / mip dims) instead of pow(2, mip) to avoid
    // fractional-pixel overshoot at the edges for non-power-of-2 image sizes.
    let scale_x = cam.img_w0 / cam.img_w;
    let scale_y = cam.img_h0 / cam.img_h;
    let img_xy = img_xy_mip0 / vec2<f32>(scale_x, scale_y);

    let bg = vec4<f32>(0.067, 0.067, 0.075, 1.0);
    if img_xy.x < 0.0 || img_xy.y < 0.0 || img_xy.x >= cam.img_w || img_xy.y >= cam.img_h {
        return bg;
    }
    
    var color = textureSample(t, s, img_xy / vec2<f32>(cam.img_w, cam.img_h));
    
    // Composite over background (both for unwritten zeroed tiles and transparent image pixels)
    color = vec4<f32>(
        color.rgb + bg.rgb * (1.0 - color.a),
        color.a + bg.a * (1.0 - color.a)
    );

    // Pixel grid — fades in smoothly from 4× to 10× zoom.
    let grid_alpha = smoothstep(4.0, 10.0, cam.zoom);
    if grid_alpha > 0.001 {
        let f = fract(img_xy_mip0);
        let line_w = 1.5 / cam.zoom;
        if f.x < line_w || f.y < line_w {
            color = mix(color, vec4<f32>(0.15, 0.15, 0.16, 1.0), grid_alpha * 0.55);
        }
    }

    return color;
}
"#;
