//! [`PixorsViewport`] — the JavaScript-facing WASM object.
//!
//! Owns the wgpu surface, device, queue, pipeline, and camera state.
//! All public methods are exported to JS via wasm-bindgen.

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;
use glam::Vec2;
use bytemuck;

use crate::camera::Camera;
use crate::context;
use crate::error::ViewportError;
use crate::pipeline;

// ── Struct ────────────────────────────────────────────────────────────────────

/// Hardware-accelerated image viewport backed by WebGL (wgpu).
///
/// # Lifecycle (JavaScript side)
/// ```js
/// const vp = await PixorsViewport.create("canvas-id");
/// vp.update_texture(width, height, rgbaBytes); // show an image
/// requestAnimationFrame(() => vp.render());    // draw a frame
/// vp.free();                                   // release GPU resources
/// ```
#[wasm_bindgen]
pub struct PixorsViewport {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Stored to avoid re-fetching from the pipeline on every `update_texture`.
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    camera_buffer: wgpu::Buffer,
    camera: Camera,
    /// Kept alive for the lifetime of the bind group that references it.
    _texture: Option<wgpu::Texture>,
    /// `None` until the first `update_texture` call.
    bind_group: Option<wgpu::BindGroup>,
}

// ── Public API (wasm-bindgen) ─────────────────────────────────────────────────

#[wasm_bindgen]
impl PixorsViewport {
    /// Initialise the GPU and attach to `canvas_id`.
    ///
    /// Async because wgpu adapter/device requests are async in the browser.
    /// Fails if WebGL is unavailable (disabled by the user or the browser).
    #[wasm_bindgen]
    pub async fn create(canvas_id: &str) -> Result<PixorsViewport, ViewportError> {
        console_error_panic_hook::set_once();

        let canvas = context::get_canvas(canvas_id)?;
        let (cw, ch) = (canvas.width(), canvas.height());

        let (surface, device, queue, config) = context::init_gpu(canvas, cw, ch).await?;

        let sampler = pipeline::create_sampler(&device);
        let bind_group_layout = pipeline::create_bind_group_layout(&device);
        let render_pipeline = pipeline::create_render_pipeline(&device, config.format, &bind_group_layout);

        let camera = Camera::new(cw as f32, ch as f32);
        let camera_buffer = Self::create_camera_buffer(&device, &camera);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            bind_group_layout,
            pipeline: render_pipeline,
            sampler,
            camera_buffer,
            camera,
            _texture: None,
            bind_group: None,
        })
    }

    /// Upload raw RGBA8 image data and reset the camera to fit the image.
    ///
    /// Must be called before the first `render`; otherwise the viewport shows
    /// only the background grey.
    #[wasm_bindgen]
    pub fn update_texture(&mut self, width: u32, height: u32, data: &[u8]) -> Result<(), ViewportError> {
        let texture = self.upload_texture(width, height, data);
        self.set_texture(texture, width, height)?;
        Ok(())
    }

    /// Create an empty texture of the given dimensions (RGBA8 sRGB).
    /// The texture is initially zeroed (black transparent).
    #[wasm_bindgen]
    pub fn create_empty_texture(&mut self, width: u32, height: u32) -> Result<(), ViewportError> {
        let max_dim = self.device.limits().max_texture_dimension_2d;
        
        if width > max_dim || height > max_dim {
            return Err(ViewportError::TextureDimensionExceeded {
                width, height, max_dim
            });
        }
        
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image_texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.set_texture(texture, width, height)?;
        web_sys::console::log_1(&"Texture created successfully".into());
        Ok(())
    }

    /// Write a tile (sub‑region) of RGBA8 pixel data into the current texture.
    /// The texture must already exist (call `create_empty_texture` or `update_texture` first).
    /// `_mip_level` is accepted for API compatibility but ignored — all data is written
    /// to GPU mip level 0 since the engine handles MIP computation server-side.
    #[wasm_bindgen]
    pub fn write_tile(&mut self, x: u32, y: u32, width: u32, height: u32, _mip_level: u32, data: &[u8]) -> Result<(), ViewportError> {
        let texture = self._texture.as_ref().ok_or(ViewportError::NoTextureCreated)?.clone();

        // Validate data size
        let expected_bytes = (width * height) as usize * 4;
        if data.len() != expected_bytes {
            return Err(ViewportError::TileDataSizeMismatch {
                expected: expected_bytes,
                got: data.len(),
            });
        }

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        Ok(())
    }

    /// Pan by a screen-space pixel delta (`dx` right, `dy` down).
    #[wasm_bindgen]
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.pan(Vec2::new(dx, dy));
        self.flush_camera();
    }

    /// Zoom by `factor` around a fixed anchor point.
    ///
    /// `anchor_x` / `anchor_y` are in screen UV space `[0,1]²` — pass the
    /// mouse position divided by canvas width/height.
    #[wasm_bindgen]
    pub fn zoom(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) {
        self.camera.zoom_at(factor, Vec2::new(anchor_x, anchor_y));
        self.flush_camera();
    }

    /// Reset the camera so the full image fits in the viewport.
    #[wasm_bindgen]
    pub fn fit(&mut self) {
        self.camera.fit();
        self.flush_camera();
    }

    /// Draw one frame to the canvas.
    #[wasm_bindgen]
    pub fn render(&mut self) -> Result<(), ViewportError> {
        let output = self.surface.get_current_texture()
            .map_err(|e| ViewportError::Internal(format!("surface texture: {e:?}")))?;

        let frame_view = output.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        self.draw_frame(&mut encoder, &frame_view);

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Notify the viewport that the canvas was resized.
    ///
    /// Call this from a `ResizeObserver` callback.  The camera adjusts
    /// automatically so the image stays correctly framed.
    #[wasm_bindgen]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.camera.viewport_size = Vec2::new(width as f32, height as f32);
        self.flush_camera();
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

impl PixorsViewport {

    /// Allocate the uniform buffer with the initial camera state.
    fn create_camera_buffer(device: &wgpu::Device, camera: &Camera) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_uniform"),
            contents: bytemuck::bytes_of(&camera.to_uniform()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Upload pixel bytes to a new GPU texture (RGBA8 sRGB).
    fn upload_texture(&self, width: u32, height: u32, data: &[u8]) -> wgpu::Texture {
        self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("image_texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        )
    }

    /// Set the current texture and update bind group & camera.
    fn set_texture(&mut self, texture: wgpu::Texture, width: u32, height: u32) -> Result<(), ViewportError> {
        let view = texture.create_view(&Default::default());

        self.bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pixors_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.camera_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        }));
        self._texture = Some(texture);

        self.camera.image_size = Vec2::new(width as f32, height as f32);
        self.camera.fit();
        self.flush_camera();
        Ok(())
    }

    /// Record a single fullscreen render pass into `encoder`.
    fn draw_frame(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pixors_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        if let Some(bg) = &self.bind_group {
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1); // fullscreen triangle — no vertex buffer
        }
    }

    /// Write the current camera state to the GPU uniform buffer.
    fn flush_camera(&mut self) {
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera.to_uniform()),
        );
    }
}
