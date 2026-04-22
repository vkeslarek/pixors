use pixors::convert::convert_raw_to_typed;
use pixors::io::png::load_png;
use pixors::viewport::{ImageView, Viewport};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::Window,
};

struct App {
    window: Option<Arc<Window>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    context: Option<Context<Arc<Window>>>,
    /// ARGB pixels of the original image (sRGB u8)
    argb_pixels: Vec<u32>,
    /// Width of the original image
    image_width: usize,
    /// Height of the original image
    image_height: usize,
    /// Viewport managing swapchain and view rectangle
    viewport: Viewport,
    /// Physical size of the window
    size: PhysicalSize<u32>,
    /// Whether left mouse button is currently pressed for dragging
    dragging: bool,
    /// Last cursor position during drag (pixors-viewport coordinates)
    last_cursor: Option<(f64, f64)>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window on resume
        let window_attributes = Window::default_attributes()
            .with_title("Pixors Display")
            .with_inner_size(LogicalSize::new(
                self.image_width as f64,
                self.image_height as f64,
            ));
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        let context = Context::new(window.clone()).unwrap();
        let mut surface = Surface::new(&context, window.clone()).unwrap();
        let size = window.inner_size();
        surface
            .resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )
            .unwrap();

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
        self.size = size;

        // Resize pixors-viewport to match window
        self.viewport.handle_resize(size.width as usize, size.height as usize);
        self.viewport.fit_image(self.image_width, self.image_height);

        // Request initial redraw
        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let window = match self.window.as_ref() {
            Some(window) => window,
            None => return,
        };

        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Create image view and render if dirty
                let image_view = ImageView::new(&self.argb_pixels, self.image_width, self.image_height);
                if self.viewport.render_if_dirty(&image_view) {
                    // If rendering occurred, flush to softbuffer
                    let surface = self.surface.as_mut().unwrap();
                    let mut buffer = surface.buffer_mut().unwrap();
                    self.viewport.flush(buffer.as_mut());
                    buffer.present().unwrap();
                } else {
                    // Still need to present the buffer (no change)
                    let surface = self.surface.as_mut().unwrap();
                    let buffer = surface.buffer_mut().unwrap();
                    buffer.present().unwrap();
                }
            }
            WindowEvent::Resized(new_size) => {
                self.size = new_size;
                let surface = self.surface.as_mut().unwrap();
                surface
                    .resize(
                        NonZeroU32::new(new_size.width).unwrap(),
                        NonZeroU32::new(new_size.height).unwrap(),
                    )
                    .unwrap();
                self.viewport.handle_resize(new_size.width as usize, new_size.height as usize);
                window.request_redraw();
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if button == MouseButton::Left {
                    self.dragging = state.is_pressed();
                    self.last_cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pos = (position.x, position.y);
                if self.dragging {
                    if let Some(last) = self.last_cursor {
                        let dx = last.0 - pos.0;
                        let dy = last.1 - pos.1;
                        // Convert screen delta to image delta
                        let scale = self.viewport.view_rect().scale();
                        self.viewport.view_rect_mut().pan(dx / scale, dy / scale);
                        window.request_redraw();
                    }
                }
                self.last_cursor = Some(pos);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Determine zoom factor
                let factor = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => {
                        if y > 0.0 { 1.1 } else { 1.0 / 1.1 }
                    }
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        if p.y > 0.0 { 1.1 } else { 1.0 / 1.1 }
                    }
                };
                // Use last known cursor position as anchor, fallback to window center
                let (anchor_x, anchor_y) = self.last_cursor.unwrap_or_else(|| {
                    (self.size.width as f64 / 2.0, self.size.height as f64 / 2.0)
                });
                self.viewport.view_rect_mut().zoom(factor, anchor_x, anchor_y);
                window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load PNG
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "example1.png".to_string());
    let raw = load_png(&std::path::Path::new(&path))?;
    println!("Loaded image: {}x{}", raw.width, raw.height);
    println!("Color space: {:?}", raw.color_space);
    println!("Alpha mode: {:?}", raw.alpha_mode);

    // Convert to internal representation (ACEScg premul f16)
    let typed = convert_raw_to_typed(raw)?;
    println!("Converted to ACEScg premul f16");

    // Convert to sRGB u8 for display (simple gamma approximation)
    let display_pixels = convert_to_srgb_u8(&typed);

    // Convert RGBA u8 to ARGB u32 (softbuffer expects ARGB)
    let argb_pixels: Vec<u32> = display_pixels
        .chunks_exact(4)
        .map(|rgba| {
            let r = rgba[0] as u32;
            let g = rgba[1] as u32;
            let b = rgba[2] as u32;
            let a = rgba[3] as u32;
            (a << 24) | (r << 16) | (g << 8) | b
        })
        .collect();

    let image_width = typed.width as usize;
    let image_height = typed.height as usize;

    let mut app = App {
        window: None,
        surface: None,
        context: None,
        argb_pixels,
        image_width,
        image_height,
        viewport: Viewport::new(2, 1, 1), // temporary size, will be resized in resumed
        size: PhysicalSize::new(0, 0),
        dragging: false,
        last_cursor: None,
    };

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app)?;

    Ok(())
}

/// Converts ACEScg premul f16 to sRGB u8 with proper color space conversion.
fn convert_to_srgb_u8(image: &pixors::TypedImage<pixors::pixel::Rgba<half::f16>>) -> Vec<u8> {
    use pixors::color::ColorSpace;
    
    // Matrix from ACEScg (AP1, D60, linear) to linear sRGB (BT.709, D65, linear)
    let mat = ColorSpace::ACES_CG.matrix_to(ColorSpace::LINEAR_SRGB)
        .expect("ACEScg to linear sRGB matrix should be valid");
    
    let mut result = Vec::with_capacity(image.pixels.len() * 4);
    const EPSILON: f32 = 1e-6;

    for pixel in &image.pixels {
        // Convert f16 to f32
        let r = pixel.r.to_f32();
        let g = pixel.g.to_f32();
        let b = pixel.b.to_f32();
        let a = pixel.a.to_f32();

        // Unpremultiply if alpha > 0
        let (r_linear, g_linear, b_linear) = if a > EPSILON {
            (r / a, g / a, b / a)
        } else {
            (0.0, 0.0, 0.0)
        };

        // Convert primaries & white point: ACEScg → linear sRGB
        let rgb_linear = mat.mul_vec([r_linear, g_linear, b_linear]);

        // Apply sRGB gamma (encode)
        let r_srgb = linear_to_srgb(rgb_linear[0]);
        let g_srgb = linear_to_srgb(rgb_linear[1]);
        let b_srgb = linear_to_srgb(rgb_linear[2]);

        // Convert to u8 (straight alpha)
        result.push((r_srgb.clamp(0.0, 1.0) * 255.0).round() as u8);
        result.push((g_srgb.clamp(0.0, 1.0) * 255.0).round() as u8);
        result.push((b_srgb.clamp(0.0, 1.0) * 255.0).round() as u8);
        result.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
    }

    result
}

/// Linear to sRGB gamma conversion (piecewise).
fn linear_to_srgb(x: f32) -> f32 {
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}