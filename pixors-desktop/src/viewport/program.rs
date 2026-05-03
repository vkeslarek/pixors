use std::sync::{Arc, Mutex};

use iced::widget::shader;
use iced::{Event, Point, Rectangle, Size};
use iced::mouse;

use crate::viewport::camera::Camera;
use crate::viewport::pipeline::{ViewportPipeline, ViewportPrimitive};
use crate::viewport::tiled_texture::TiledTexture;

pub struct ViewportProgram {
    pub tiled_texture: Option<Arc<Mutex<TiledTexture>>>,
    pub pending_writes: Arc<PendingTileWrites>,
    pub camera: Arc<Mutex<Camera>>,
}

/// Buffers tile writes between `App` (UI thread) and `prepare` (render thread).
pub struct PendingTileWrites {
    pub queue: Mutex<Vec<PendingTile>>,
    pub realloc: Mutex<Option<(u32, u32)>>,
}

pub struct PendingTile {
    pub px: u32,
    pub py: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub bytes: Vec<u8>,
}

impl<Msg> shader::Program<Msg> for ViewportProgram {
    type State = ViewportState;
    type Primitive = ViewportPrimitive;

    fn draw(
        &self,
        state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        ViewportPrimitive {
            camera: state.camera_uniform,
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        let mut cam = self.camera.lock().unwrap();
        let size = Size::new(bounds.width, bounds.height);

        if state.last_bounds.map_or(true, |s| s != size) {
            if !state.fitted {
                cam.resize(size.width, size.height);
                cam.fit();
                state.fitted = true;
            } else {
                cam.resize(size.width, size.height);
            }
            state.last_bounds = Some(size);
        }

        let action = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.position_in(bounds).is_some() {
                    state.dragging = true;
                    state.last_pos = cursor.position_in(bounds);
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                state.last_pos = None;
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(curr) = cursor.position_in(bounds) {
                        if let Some(last) = state.last_pos {
                            let dx = curr.x - last.x;
                            let dy = curr.y - last.y;
                            cam.pan(dx, dy);
                        }
                        state.last_pos = Some(curr);
                    }
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_some() {
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y * 24.0,
                        mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    let factor = if dy > 0.0 {
                        1.1_f32.powf(dy)
                    } else {
                        1.0 / 1.1_f32.powf(-dy)
                    };
                    let pos = cursor
                        .position_in(bounds)
                        .unwrap_or(Point::new(0.0, 0.0));
                    cam.zoom_at(factor, pos.x, pos.y);
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        };

        state.camera_uniform = cam.to_uniform();
        drop(cam);
        action
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::default()
        }
    }
}

pub struct ViewportState {
    camera_uniform: crate::viewport::camera::CameraUniform,
    dragging: bool,
    fitted: bool,
    last_pos: Option<Point>,
    last_bounds: Option<Size>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera_uniform: Camera::new(2048.0, 1536.0).to_uniform(),
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
        }
    }
}
