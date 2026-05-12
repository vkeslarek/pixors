mod app;
pub mod components;
mod controller;
mod document_view;
mod effect;
mod icons;
pub mod layout;
pub mod modal;
pub mod page;
pub mod panel;
mod theme;
mod viewport;

use app::App;

const SPACE_GROTESK: &[u8] = include_bytes!("../assets/space-grotesk-variable-latin.ttf");
const SPACE_MONO_400: &[u8] = include_bytes!("../assets/space-mono-400-latin.ttf");
const SPACE_MONO_700: &[u8] = include_bytes!("../assets/space-mono-700-latin.ttf");

fn main() -> iced::Result {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .add_directive("cosmic_text=info".parse().unwrap())
                .add_directive("wgpu_core=warn".parse().unwrap())
                .add_directive("wgpu_hal=warn".parse().unwrap())
                .add_directive("naga=warn".parse().unwrap())
                .add_directive("iced_wgpu=info".parse().unwrap())
                .add_directive("iced_winit=info".parse().unwrap()),
        )
        .init();

    iced::application(App::default, App::update, App::view)
        .subscription(App::subscription)
        .title("Pixors")
        .window_size((1280.0, 720.0))
        .font(icons::FONT_BYTES)
        .font(SPACE_GROTESK)
        .font(SPACE_MONO_400)
        .font(SPACE_MONO_700)
        .default_font(iced::Font::with_name("Space Grotesk Light"))
        .theme(iced::Theme::Dark)
        .run()
}
