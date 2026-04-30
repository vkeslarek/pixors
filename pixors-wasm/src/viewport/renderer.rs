use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

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
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });

        web_sys::console::log_1(&JsValue::from_str("WebGPU: Requesting adapter..."));

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
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| format!("device: {}", e))?;

        web_sys::console::log_1(&JsValue::from_str("WebGPU: Device acquired"));

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let (texture, buffer, padded_bytes_per_row) =
            Self::create_render_targets(&device, width, height);

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
        })
    }

    fn create_render_targets(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::Buffer, u32) {
        let w = width.max(1);
        let h = height.max(1);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Render Texture"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
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
        if let Some((width, height)) = inner.pending_resize.take() {
            if width == 0 || height == 0 || (width == inner.width && height == inner.height) {
                return;
            }
            inner.width = width;
            inner.height = height;

            let (texture, buffer, padded_bytes_per_row) =
                Self::create_render_targets(&self.device, width, height);
            inner.texture = texture;
            inner.buffer = buffer;
            inner.padded_bytes_per_row = padded_bytes_per_row;
        }
    }

    pub async fn render(&self) -> Result<js_sys::Uint8Array, JsValue> {
        self.apply_resize();

        let map_state: Rc<RefCell<MapState>> =
            Rc::new(RefCell::new(MapState::Pending(None)));

        {
            let inner = self.inner.borrow();
            let view = inner
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            {
                let _rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.8,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }

            encoder.copy_texture_to_buffer(
                wgpu::ImageCopyTexture {
                    texture: &inner.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyBuffer {
                    buffer: &inner.buffer,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(inner.padded_bytes_per_row),
                        rows_per_image: Some(inner.height),
                    },
                },
                wgpu::Extent3d {
                    width: inner.width.max(1),
                    height: inner.height.max(1),
                    depth_or_array_layers: 1,
                },
            );

            self.queue.submit(std::iter::once(encoder.finish()));

            let state_cb = map_state.clone();
            inner
                .buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |res| {
                    let prev =
                        std::mem::replace(&mut *state_cb.borrow_mut(), MapState::Done(res));
                    if let MapState::Pending(Some(w)) = prev {
                        w.wake();
                    }
                });
        }

        MapFuture { state: map_state }
            .await
            .map_err(|e| JsValue::from_str(&format!("map: {:?}", e)))?;

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

enum MapState {
    Pending(Option<std::task::Waker>),
    Done(Result<(), wgpu::BufferAsyncError>),
}

struct MapFuture {
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
                if let MapState::Done(r) =
                    std::mem::replace(&mut *s, MapState::Pending(None))
                {
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
