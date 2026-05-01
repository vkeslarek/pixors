mod ui;
mod engine;
mod viewport;

use ui::app::App;

fn main() -> iced::Result {
    iced::application("Pixors Ultra Test", App::update, App::view)
        .theme(|_| iced::Theme::Dark)
        .window_size((1280.0, 720.0))
        .font(ui::icons::FONT_BYTES)
        .default_font(iced::Font::DEFAULT)
        .run()
}
