use std::sync::Arc;
use tao::{
    dpi::{LogicalPosition, LogicalSize, Size},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{Rect, WebViewBuilder};

fn main() -> wry::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
    {
        use gtk::prelude::DisplayExtManual;
        gtk::init().unwrap();
        if gtk::gdk::Display::default().unwrap().backend().is_wayland() {
            panic!("Wayland not supported yet — use X11");
        }
    }

    let event_loop = EventLoop::new();
    let window = Arc::new(
        WindowBuilder::new()
            .with_decorations(false)
            .with_title("Pixors")
            .with_inner_size(Size::Logical(LogicalSize::new(1440.0, 900.0)))
            // .with_min_inner_size(Size::Logical(LogicalSize::new(640.0, 480.0)))
            .with_resizable(true)
            .build(&event_loop)
            .unwrap()
    );

    let w_ref = Arc::clone(&window);
    let builder = WebViewBuilder::new()
        .with_url("http://localhost:5173/")
        .with_devtools(true)
        .with_autoplay(true)
        .with_drag_drop_handler(|_| false)
        .with_initialization_script(include_str!("./bridge.js"))
        .with_ipc_handler(move |msg| {
            let body: &str = msg.body();
            match body {
                "minimize" => { w_ref.set_minimized(true); }
                "maximize" => { w_ref.set_maximized(!w_ref.is_maximized()); }
                "drag_window" => { let _ = w_ref.drag_window(); }
                "close" => std::process::exit(0),
                _ => {}
            };
        });

    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().unwrap();
        builder.build_gtk(vbox)?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&*window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                let s = size.to_logical::<u32>(window.scale_factor());
                let _ = webview.set_bounds(Rect {
                    position: LogicalPosition::new(0, 0).into(),
                    size: LogicalSize::new(s.width, s.height).into(),
                });
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}
