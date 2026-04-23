use crate::error::ViewportError;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

/// Look up a `<canvas>` element in the DOM by ID.
pub fn get_canvas(canvas_id: &str) -> Result<HtmlCanvasElement, ViewportError> {
    web_sys::window()
        .expect("no global window")
        .document()
        .expect("no document on window")
        .get_element_by_id(canvas_id)
        .ok_or_else(|| ViewportError::CanvasNotFound(canvas_id.to_string()))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| ViewportError::CanvasNotFound(format!("#{canvas_id} is not a <canvas>")))
}

/// Create wgpu surface, adapter, device, queue, and configure the surface.
pub async fn init_gpu(
    canvas: HtmlCanvasElement,
    width: u32,
    height: u32,
) -> Result<(wgpu::Surface<'static>, wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration), ViewportError> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL, // WebGL — widest browser support
        ..Default::default()
    });

    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| ViewportError::Internal(format!("surface: {e:?}")))?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .ok_or(ViewportError::NoWebGlSupport)?;

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
        .map_err(|_| ViewportError::DeviceRequestFailed)?;

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
