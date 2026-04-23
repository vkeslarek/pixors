use dpi::{LogicalPosition, LogicalSize};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use wry::{Rect, WebViewBuilder};

#[derive(Default)]
struct State {
    window: Option<Window>,
    webview: Option<wry::WebView>,
}

impl ApplicationHandler for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut attributes = Window::default_attributes();
        attributes.inner_size = Some(LogicalSize::new(1440, 900).into());
        attributes.min_inner_size = Some(LogicalSize::new(1280, 800).into());
        attributes.title = "Pixors".to_string();
        attributes.resizable = true;
        // TODO: Set window icon when we have PNG/ICO files
        let window = event_loop.create_window(attributes).unwrap();

        let webview = WebViewBuilder::new()
            .with_url("http://localhost:5173/")
            .with_devtools(false)
            .with_autoplay(true)
            .with_initialization_script(
                r#"
                // Disable text selection
                document.addEventListener('DOMContentLoaded', () => {
                    document.body.style.userSelect = 'none';
                    document.body.style.webkitUserSelect = 'none';
                    // Prevent drag-and-drop of images
                    document.addEventListener('dragstart', (e) => e.preventDefault());
                    // Prevent right-click context menu
                    document.addEventListener('contextmenu', (e) => e.preventDefault());
                });
                "#,
            )
            .build_as_child(&window)
            .unwrap();

        self.window = Some(window);
        self.webview = Some(webview);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                let window = self.window.as_ref().unwrap();
                let webview = self.webview.as_ref().unwrap();

                let size = size.to_logical::<u32>(window.scale_factor());
                webview
                    .set_bounds(Rect {
                        position: LogicalPosition::new(0, 0).into(),
                        size: LogicalSize::new(size.width, size.height).into(),
                    })
                    .unwrap();
            }
            WindowEvent::CloseRequested => {
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
        ))]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(16),
        ));
    }
}

fn main() -> wry::Result<()> {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    {
        use gtk::prelude::DisplayExtManual;

        gtk::init().unwrap();
        if gtk::gdk::Display::default().unwrap().backend().is_wayland() {
            panic!("This example doesn't support wayland!");
        }

        winit::platform::x11::register_xlib_error_hook(Box::new(|_display, error| {
            let error = error as *mut x11_dl::xlib::XErrorEvent;
            (unsafe { (*error).error_code }) == 170
        }));
    }

    let event_loop = EventLoop::new().unwrap();
    let mut state = State::default();
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}
