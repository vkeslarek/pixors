use std::sync::{Arc, Mutex};

use iced::widget::shader;
use iced::{Event, Point, Rectangle, Size};
use iced::mouse;

use crate::viewport::camera::Camera;
use crate::viewport::pipeline::ViewportPrimitive;

pub struct ViewportProgram {
    pub pending_writes: Arc<PendingTileWrites>,
}

pub struct PendingTileWrites {
    pub queue: Mutex<Vec<PendingTile>>,
    pub realloc: Mutex<Option<(u32, u32)>>,
    pub new_img: Mutex<Option<(u32, u32)>>,
}

impl PendingTileWrites {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(Vec::new()),
            realloc: Mutex::new(None),
            new_img: Mutex::new(None),
        })
    }

    pub fn signal_realloc(&self, w: u32, h: u32) {
        *self.realloc.lock().unwrap() = Some((w, h));
    }

    pub(super) fn take_new_img(&self) -> Option<(u32, u32)> {
        self.new_img.lock().unwrap().take()
    }

    pub fn push_tile(&self, tile: PendingTile) {
        self.queue.lock().unwrap().push(tile);
    }

    /// Takes the pending realloc dimensions if any.
    pub(super) fn take_realloc(&self) -> Option<(u32, u32)> {
        self.realloc.lock().unwrap().take()
    }

    /// Drains all queued tiles. Lock is released before the Vec is returned.
    pub(super) fn drain_tiles(&self) -> Vec<PendingTile> {
        self.queue.lock().unwrap().drain(..).collect()
    }

    /// Tile an RGBA8 image and enqueue all tiles for GPU upload.
    pub fn load_image(&self, rgba: &[u8], w: u32, h: u32) {
        const TILE: u32 = 256;
        self.signal_realloc(w, h);
        *self.new_img.lock().unwrap() = Some((w, h));
        let stride = w as usize * 4;
        for ty in 0..h.div_ceil(TILE) {
            for tx in 0..w.div_ceil(TILE) {
                let px = tx * TILE;
                let py = ty * TILE;
                let tw = (w - px).min(TILE) as usize;
                let th = (h - py).min(TILE) as usize;
                let mut bytes = vec![0u8; tw * th * 4];
                for row in 0..th {
                    let src = (py as usize + row) * stride + px as usize * 4;
                    let dst = row * tw * 4;
                    bytes[dst..dst + tw * 4].copy_from_slice(&rgba[src..src + tw * 4]);
                }
                self.push_tile(PendingTile {
                    px,
                    py,
                    tile_w: tw as u32,
                    tile_h: th as u32,
                    bytes,
                });
            }
        }
    }
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
            camera: state.camera.to_uniform(),
            pending_writes: self.pending_writes.clone(),
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        // New image loaded — update camera dims and re-fit regardless of bounds change.
        if let Some((img_w, img_h)) = self.pending_writes.take_new_img() {
            state.camera.img_w = img_w as f32;
            state.camera.img_h = img_h as f32;
            state.camera.fit();
        }

        let size = Size::new(bounds.width, bounds.height);
        if state.last_bounds != Some(size) {
            state.camera.resize(size.width, size.height);
            if !state.fitted {
                state.camera.fit();
                state.fitted = true;
            }
            state.last_bounds = Some(size);
        }

        match event {
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
                            state.camera.pan(curr.x - last.x, curr.y - last.y);
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
                    // Normalize to "steps": 1 mouse wheel click = 1 step, trackpad pixels / 16.
                    let steps = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y,
                        mouse::ScrollDelta::Pixels { y, .. } => y / 16.0,
                    };
                    let factor = 1.15_f32.powf(steps.clamp(-5.0, 5.0));
                    let pos = cursor.position_in(bounds).unwrap_or(Point::ORIGIN);
                    state.camera.zoom_at(factor, pos.x, pos.y);
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        }
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
    pub(super) camera: Camera,
    dragging: bool,
    fitted: bool,
    last_pos: Option<Point>,
    last_bounds: Option<Size>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera: Camera::new(1.0, 1.0),
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
        }
    }
}
