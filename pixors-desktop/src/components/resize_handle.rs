use iced::advanced::layout::{self, Layout, Node};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Renderer as _, Shell, mouse};
use iced::{Background, Border, Color, Element, Event, Length, Rectangle, Size, Theme};

const HANDLE_WIDTH: f32 = 6.0;
const HIT_WIDTH: f32 = 12.0;

#[derive(Default)]
struct State {
    dragging: bool,
    last_x: f32,
}

pub struct ResizeHandle<Message> {
    on_resize: Box<dyn Fn(f32) -> Message>,
}

impl<Message> ResizeHandle<Message> {
    pub fn new(on_resize: impl Fn(f32) -> Message + 'static) -> Self {
        Self {
            on_resize: Box::new(on_resize),
        }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for ResizeHandle<Message> {
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(HIT_WIDTH), Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> Node {
        let limits = limits.width(HIT_WIDTH).height(Length::Fill);
        Node::new(limits.resolve(HIT_WIDTH, Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.dragging = true;
                    if let Some(pos) = cursor.position() {
                        state.last_x = pos.x;
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.dragging {
                    let delta = position.x - state.last_x;
                    state.last_x = position.x;
                    shell.publish((self.on_resize)(delta));
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.dragging || cursor.is_over(layout.bounds()) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let hovered = cursor.is_over(bounds) || state.dragging;

        // Thin visible line centered in the hit area
        let line_x = bounds.x + (bounds.width - HANDLE_WIDTH) / 2.0;
        let line_bounds = Rectangle {
            x: line_x,
            y: bounds.y,
            width: HANDLE_WIDTH,
            height: bounds.height,
        };

        let bg_alpha = if state.dragging {
            0.25
        } else if hovered {
            0.15
        } else {
            0.06
        };

        renderer.fill_quad(
            iced::advanced::renderer::Quad {
                bounds: line_bounds,
                border: Border::default().rounded(2),
                ..Default::default()
            },
            Background::Color(Color::from_rgba(1.0, 1.0, 1.0, bg_alpha)),
        );

        // Grip indicator (3 dots) on hover
        if hovered {
            let dot_size = 2.0;
            let gap = 6.0;
            let cx = line_bounds.x + line_bounds.width / 2.0 - dot_size / 2.0;
            let cy = line_bounds.y + line_bounds.height / 2.0;
            for i in [-1.0_f32, 0.0, 1.0] {
                let dot = Rectangle {
                    x: cx,
                    y: cy + i * gap - dot_size / 2.0,
                    width: dot_size,
                    height: dot_size,
                };
                renderer.fill_quad(
                    iced::advanced::renderer::Quad {
                        bounds: dot,
                        border: Border::default().rounded(1),
                        ..Default::default()
                    },
                    Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.5)),
                );
            }
        }
    }
}

impl<'a, Message: 'a> From<ResizeHandle<Message>> for Element<'a, Message> {
    fn from(handle: ResizeHandle<Message>) -> Self {
        Element::new(handle)
    }
}

pub fn resize_handle<Message: 'static>(
    on_resize: impl Fn(f32) -> Message + 'static,
) -> Element<'static, Message> {
    ResizeHandle::new(on_resize).into()
}
