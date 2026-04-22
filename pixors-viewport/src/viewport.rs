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
    pub async fn create(canvas_id: &str) -> Result<PixorsViewport, JsValue> {
        console_error_panic_hook::set_once();

        let canvas = Self::get_canvas(canvas_id)?;
        let (cw, ch) = (canvas.width(), canvas.height());

        let (surface, device, queue, config) = Self::init_gpu(canvas, cw, ch).await?;

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
    pub fn update_texture(&mut self, width: u32, height: u32, data: &[u8]) -> Result<(), JsValue> {
        let texture = self.upload_texture(width, height, data);
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
    pub fn render(&mut self) -> Result<(), JsValue> {
        let output = self.surface.get_current_texture()
            .map_err(|e| JsValue::from_str(&format!("surface texture: {e:?}")))?;

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
    /// Look up a `<canvas>` element in the DOM by ID.
    fn get_canvas(canvas_id: &str) -> Result<HtmlCanvasElement, JsValue> {
        web_sys::window()
            .expect("no global window")
            .document()
            .expect("no document on window")
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str(&format!("canvas #{canvas_id} not found")))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str(&format!("#{canvas_id} is not a <canvas>")))
    }

    /// Create wgpu surface, adapter, device, queue, and configure the surface.
    async fn init_gpu(
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<(wgpu::Surface<'static>, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration), JsValue> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL, // WebGL — widest browser support
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&format!("surface: {e:?}")))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| JsValue::from_str(
                "No WebGL adapter found. Enable hardware acceleration in your browser settings."
            ))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: adapter.limits(),
                },
                None,
            )
            .await
            .map_err(|e| JsValue::from_str(&format!("device: {e:?}")))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok((surface, device, queue, config))
    }

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

    /// Record a single fullscreen render pass into `encoder`.
    fn draw_frame(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pixors_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Background grey shown for areas outside the image.
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.18, g: 0.18, b: 0.18, a: 1.0 }),
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
