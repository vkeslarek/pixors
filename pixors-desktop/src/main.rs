mod ui;
mod engine;
mod viewport;

use ui::app::App;

const SPACE_GROTESK: &[u8] = include_bytes!("../assets/space-grotesk-variable-latin.ttf");
const SPACE_MONO_400: &[u8] = include_bytes!("../assets/space-mono-400-latin.ttf");
const SPACE_MONO_700: &[u8] = include_bytes!("../assets/space-mono-700-latin.ttf");

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();
    iced::application(App::default, App::update, App::view)
        .title("Pixors Ultra Test")
        .window_size((1280.0, 720.0))
        .font(ui::icons::FONT_BYTES)
        .font(SPACE_GROTESK)
        .font(SPACE_MONO_400)
        .font(SPACE_MONO_700)
        .default_font(iced::Font::with_name("Space Grotesk"))
        .theme(iced::Theme::Dark)
        .run()
}
