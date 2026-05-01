mod color;
mod pixel;
mod image;
mod pipeline;
mod error;
mod approx;
mod composite;
mod io;
mod storage;
mod utils;
mod ui;
mod engine;
mod viewport;

use ui::app::App;

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();
    iced::application(App::default, App::update, App::view)
        .title("Pixors Ultra Test")
        .window_size((1280.0, 720.0))
        .font(ui::icons::FONT_BYTES)
        .default_font(iced::Font::DEFAULT)
        .theme(iced::Theme::Dark)
        .run()
}
