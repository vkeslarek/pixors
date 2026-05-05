use std::sync::{Arc, Mutex};

use iced::widget::shader;
use iced::{Event, Point, Rectangle, Size};
use iced::mouse;
use pixors_executor::source::TileRange;

use crate::viewport::camera::Camera;
use crate::viewport::pipeline::ViewportPrimitive;
use crate::viewport::tile_cache::ViewportCache;

pub const TILE_SIZE: u32 = 256;

pub struct ViewportProgram {
    pub cache: Option<Arc<Mutex<ViewportCache>>>,
    pub tile_generation: u64,
    /// Set by update() when MIP changes: (mip_level, visible_tile_range).
    /// App polls this on each tick to trigger a disk fetch if needed.
    pub mip_fetch_signal: Arc<Mutex<Vec<(u32, TileRange)>>>,
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
            camera: state.camera.to_uniform(state.current_mip),
            cache: self.cache.clone(),
        }
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        if let Some(ref cache) = self.cache {
            if let Ok(mut guard) = cache.lock() {
                if let Some((img_w, img_h)) = guard.take_new_img() {
                    state.camera.img_w = img_w as f32;
                    state.camera.img_h = img_h as f32;
                    state.camera.fit();
                    state.current_mip = state.camera.visible_mip_level();
                }
            }
        }

        let size = Size::new(bounds.width, bounds.height);
        if state.last_bounds != Some(size) {
            state.camera.resize(size.width, size.height);
            if !state.fitted {
                state.camera.fit();
                state.fitted = true;
                state.current_mip = state.camera.visible_mip_level();
            }
            state.last_bounds = Some(size);
        }

        let old_mip = state.current_mip;

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
                    let steps = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y,
                        mouse::ScrollDelta::Pixels { y, .. } => y / 16.0,
                    };
                    let factor = 1.15_f32.powf(steps.clamp(-5.0, 5.0));
                    let pos =
                        cursor.position_in(bounds).unwrap_or(Point::ORIGIN);
                    state.camera.zoom_at(factor, pos.x, pos.y);
                    state.current_mip = state.camera.visible_mip_level();
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        };

        if state.current_mip != old_mip {
            tracing::info!(
                "[pixors] viewport: MIP changed {} → {}",
                old_mip,
                state.current_mip,
            );
            let mut reqs = Vec::new();
            reqs.push((state.current_mip, state.camera.visible_tile_range(state.current_mip, TILE_SIZE)));
            
            // Preemptively fetch lower resolution (zoomed out, MIP + 1)
            let max_mip = crate::viewport::camera::compute_max_mip(state.camera.img_w as u32, state.camera.img_h as u32);
            if state.current_mip < max_mip {
                reqs.push((state.current_mip + 1, state.camera.visible_tile_range(state.current_mip + 1, TILE_SIZE)));
            }
            // Preemptively fetch higher resolution (zoomed in, MIP - 1)
            if state.current_mip > 0 {
                reqs.push((state.current_mip - 1, state.camera.visible_tile_range(state.current_mip - 1, TILE_SIZE)));
            }
            
            *self.mip_fetch_signal.lock().unwrap() = reqs;
            Some(shader::Action::request_redraw().and_capture())
        } else {
            action
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
    pub(super) current_mip: u32,
    dragging: bool,
    fitted: bool,
    last_pos: Option<Point>,
    last_bounds: Option<Size>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera: Camera::new(1.0, 1.0),
            current_mip: 0,
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
        }
    }
}
