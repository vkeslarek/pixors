mod embedded_ui;

use std::borrow::Cow;
use std::sync::Arc;
use tao::{
    dpi::{LogicalSize, PhysicalSize, Size},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{CursorIcon, ResizeDirection, Window, WindowBuilder},
};
use wry::{
    http::{header, Request, Response, StatusCode},
    WebViewBuilder,
};

#[derive(Debug)]
enum HitTestResult {
    Client, Left, Right, Top, Bottom,
    TopLeft, TopRight, BottomLeft, BottomRight, NoWhere,
}

impl HitTestResult {
    fn drag_resize_window(&self, window: &Window) {
        let _ = window.drag_resize_window(match self {
            HitTestResult::Left => ResizeDirection::West,
            HitTestResult::Right => ResizeDirection::East,
            HitTestResult::Top => ResizeDirection::North,
            HitTestResult::Bottom => ResizeDirection::South,
            HitTestResult::TopLeft => ResizeDirection::NorthWest,
            HitTestResult::TopRight => ResizeDirection::NorthEast,
            HitTestResult::BottomLeft => ResizeDirection::SouthWest,
            HitTestResult::BottomRight => ResizeDirection::SouthEast,
            _ => unreachable!(),
        });
    }

    fn change_cursor(&self, window: &Window) {
        window.set_cursor_icon(match self {
            HitTestResult::Left => CursorIcon::WResize,
            HitTestResult::Right => CursorIcon::EResize,
            HitTestResult::Top => CursorIcon::NResize,
            HitTestResult::Bottom => CursorIcon::SResize,
            HitTestResult::TopLeft => CursorIcon::NwResize,
            HitTestResult::TopRight => CursorIcon::NeResize,
            HitTestResult::BottomLeft => CursorIcon::SwResize,
            HitTestResult::BottomRight => CursorIcon::SeResize,
            _ => CursorIcon::Default,
        });
    }
}

fn hit_test(window_size: PhysicalSize<u32>, x: i32, y: i32, scale: f64) -> HitTestResult {
    const INSET: f64 = 5.0;
    let inset = (INSET * scale) as i32;
    let w = window_size.width as i32;
    let h = window_size.height as i32;

    let left = (x < inset) as i32;
    let right = (x >= w - inset) as i32;
    let top = (y < inset) as i32;
    let bottom = (y >= h - inset) as i32;

    match (left, right, top, bottom) {
        (0, 0, 0, 0) => HitTestResult::Client,
        (1, 0, 0, 0) => HitTestResult::Left,
        (0, 1, 0, 0) => HitTestResult::Right,
        (0, 0, 1, 0) => HitTestResult::Top,
        (0, 0, 0, 1) => HitTestResult::Bottom,
        (1, 0, 1, 0) => HitTestResult::TopLeft,
        (0, 1, 1, 0) => HitTestResult::TopRight,
        (1, 0, 0, 1) => HitTestResult::BottomLeft,
        (0, 1, 0, 1) => HitTestResult::BottomRight,
        _ => HitTestResult::NoWhere,
    }
}

#[derive(Debug)]
enum UserEvent {
    Minimize,
    Maximize,
    DragWindow,
    CloseWindow,
    MouseDown(i32, i32),
    MouseMove(i32, i32),
}

fn serve_asset(path: &str) -> Response<Cow<'static, [u8]>> {
    let file = embedded_ui::get(path);
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut res = Response::new(file);
    res.headers_mut()
        .insert(header::CONTENT_TYPE, mime.as_ref().parse().unwrap());
    res.headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    *res.status_mut() = StatusCode::OK;
    res
}

fn main() -> wry::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
    {
        use gtk::prelude::DisplayExtManual;
        gtk::init().unwrap();
        if gtk::gdk::Display::default().unwrap().backend().is_wayland() {
            panic!("Wayland not supported yet — use X11");
        }
    }

    // Start engine WebSocket server in background
    pixors_engine::server::start_server_bg(pixors_engine::config::Config::default());

    let webview_url = if std::env::var("PIXORS_DEV").is_ok() {
        "http://localhost:5173/".to_string()
    } else {
        "pixors://localhost/index.html".to_string()
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = Arc::new(
        WindowBuilder::new()
            .with_decorations(false)
            .with_title("Pixors")
            .with_inner_size(Size::Logical(LogicalSize::new(1440.0, 900.0)))
            .with_min_inner_size(Size::Logical(LogicalSize::new(640.0, 480.0)))
            .with_resizable(true)
            .build(&event_loop)
            .unwrap()
    );

    let proxy = event_loop.create_proxy();
    let handler = move |req: Request<String>| {
        let body = req.body();
        let mut parts = body.split([':', ',']);
        let _ = match parts.next().unwrap() {
            "minimize"     => proxy.send_event(UserEvent::Minimize),
            "maximize"     => proxy.send_event(UserEvent::Maximize),
            "drag_window"  => proxy.send_event(UserEvent::DragWindow),
            "close"        => proxy.send_event(UserEvent::CloseWindow),
            "mousedown" => {
                let x = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let y = parts.next().unwrap_or("0").parse().unwrap_or(0);
                proxy.send_event(UserEvent::MouseDown(x, y))
            }
            "mousemove" => {
                let x = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let y = parts.next().unwrap_or("0").parse().unwrap_or(0);
                proxy.send_event(UserEvent::MouseMove(x, y))
            }
            _ => Ok(()),
        };
    };

    let builder = WebViewBuilder::new()
        .with_url(&webview_url)
        .with_devtools(true)
        .with_autoplay(true)
        .with_custom_protocol("pixors".into(), move |_id, req: Request<Vec<u8>>| {
            let path = req.uri().path();
            let path = path.strip_prefix('/').unwrap_or(path);
            let path = if path.is_empty() { "index.html" } else { path };
            serve_asset(path)
        })
        .with_initialization_script(include_str!("./bridge.js"))
        .with_ipc_handler(handler)
        .with_accept_first_mouse(true);

    #[cfg(target_os = "linux")]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().unwrap();
        builder.build_gtk(vbox)?
    };
    #[cfg(not(target_os = "linux"))]
    let _webview = builder.build(&*window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::CloseWindow) => {
                *control_flow = ControlFlow::Exit
            }

            Event::UserEvent(e) => match e {
                UserEvent::Minimize => window.set_minimized(true),
                UserEvent::Maximize => window.set_maximized(!window.is_maximized()),
                UserEvent::DragWindow => { let _ = window.drag_window(); }
                UserEvent::MouseDown(x, y) => {
                    let res = hit_test(window.inner_size(), x, y, window.scale_factor());
                    match res {
                        HitTestResult::Client | HitTestResult::NoWhere => {}
                        _ => res.drag_resize_window(&window),
                    }
                }
                UserEvent::MouseMove(x, y) => {
                    hit_test(window.inner_size(), x, y, window.scale_factor()).change_cursor(&window);
                }
                UserEvent::CloseWindow => {}
            },
            _ => {}
        }
    })
}
