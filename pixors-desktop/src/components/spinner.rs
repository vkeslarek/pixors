use iced::widget::Canvas;
use iced::widget::canvas::{self, Frame, Path};
use iced::{Color, Element, Length, Point, Rectangle};

use crate::theme::ACCENT;

pub struct Spinner<Message> {
    size: f32,
    width: f32,
    color: Color,
    frame: u64,
    _phantom: std::marker::PhantomData<Message>,
}

pub fn spinner<Message>(frame: u64) -> Spinner<Message> {
    Spinner {
        size: 24.0,
        width: 2.0,
        color: ACCENT,
        frame,
        _phantom: std::marker::PhantomData,
    }
}

impl<Message> Spinner<Message> {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl<Message> canvas::Program<Message> for Spinner<Message> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let cx = bounds.size().width / 2.0;
        let cy = bounds.size().height / 2.0;
        let r = self.size / 2.0;
        let n = 3.0_f32;
        let angle_step = std::f32::consts::TAU / n;
        let base = (self.frame as f32 * 0.08) % std::f32::consts::TAU;

        for i in 0..3 {
            let a = base + i as f32 * angle_step;
            let x = cx + a.cos() * r * 0.6;
            let y = cy + a.sin() * r * 0.6;
            let alpha = 0.3 + 0.7 * (1.0 - i as f32 / n);
            let mut c = self.color;
            c.a = alpha;

            frame.fill(
                &Path::circle(Point::new(x, y), self.width * 1.2 + i as f32 * 0.3),
                c,
            );
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        _state: &mut Self::State,
        _event: &canvas::Event,
        _bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        Some(canvas::Action::request_redraw())
    }
}

impl<'a, Message: 'static> From<Spinner<Message>> for Element<'a, Message> {
    fn from(spinner: Spinner<Message>) -> Self {
        let s = spinner.size * 2.0 + spinner.width * 4.0;
        Canvas::new(spinner)
            .width(Length::Fixed(s))
            .height(Length::Fixed(s))
            .into()
    }
}
