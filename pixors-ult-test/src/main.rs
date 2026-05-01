mod color;
mod pixel;
mod image;
mod pipeline;
mod error;
mod approx;
mod ui;
mod engine;
mod viewport;

use ui::app::App;

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Pixors Ultra Test")
        .window_size((1280.0, 720.0))
        .font(ui::icons::FONT_BYTES)
        .default_font(iced::Font::DEFAULT)
        .theme(iced::Theme::Dark)
        .run()
}
