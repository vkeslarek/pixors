use std::sync::{Arc, Mutex};

use iced::widget::shader;
use iced::{Event, Point, Rectangle, Size};
use iced::mouse;
use pixors_executor::source::cache_reader::TileRange;

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
    type State = std::cell::RefCell<ViewportState>;
    type Primitive = ViewportPrimitive;

    fn draw(
        &self,
        state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let mut state = state.borrow_mut();

        let old_mip = state.current_mip;

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

        let mut target_mip = state.camera.visible_mip_level();

        // Fallback to lower MIPs (higher resolution) if the target MIP hasn't generated enough tiles yet.
        // This allows progressively showing the image during initial load.
        if let Some(ref cache) = self.cache {
            if let Ok(guard) = cache.lock() {
                let base_mip = state.camera.floor_mip();
                if target_mip > base_mip && !guard.has_mip(target_mip) && guard.has_mip(base_mip) {
                    tracing::info!("[pixors] viewport: fallback from target {} to mip {}", target_mip, base_mip);
                    target_mip = base_mip;
                }
            }
        }
        
        if state.current_mip != target_mip {
            tracing::info!("[pixors] viewport: draw() setting current_mip to {}", target_mip);
        }
        state.current_mip = target_mip;

        let mut reqs = Vec::new();
        // Fetch the primary mip level with aggressive padding (3 tiles) for panning
        reqs.push((state.current_mip, state.camera.padded_tile_range(state.current_mip, TILE_SIZE, 3)));
        
        // Preemptively fetch lower resolution (zoomed out, MIP + 1), padding 2
        let max_mip = crate::viewport::camera::compute_max_mip(state.camera.img_w as u32, state.camera.img_h as u32);
        if state.current_mip < max_mip {
            reqs.push((state.current_mip + 1, state.camera.padded_tile_range(state.current_mip + 1, TILE_SIZE, 2)));
        }
        // Preemptively fetch higher resolution (zoomed in, MIP - 1), padding 2
        if state.current_mip > 0 {
            reqs.push((state.current_mip - 1, state.camera.padded_tile_range(state.current_mip - 1, TILE_SIZE, 2)));
        }

        if Some(reqs.clone()) != state.last_reqs {
            if state.current_mip != old_mip {
                tracing::info!(
                    "[pixors] viewport: MIP changed {} → {}",
                    old_mip,
                    state.current_mip,
                );
            }

            if let Ok(mut sig) = self.mip_fetch_signal.lock() {
                *sig = reqs.clone();
            }
            state.last_reqs = Some(reqs);
        }

        ViewportPrimitive {
            camera: state.camera.to_uniform(state.current_mip),
            cache: self.cache.clone(),
            visible_range: state.camera.padded_tile_range(state.current_mip, TILE_SIZE, 3),
        }
    }

    fn update(
        &self,
        state_cell: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        let state = state_cell.get_mut();

        if self.tile_generation != state.last_generation.get() {
            tracing::info!("[pixors] viewport: update() saw generation change ({} -> {}), requesting redraw", state.last_generation.get(), self.tile_generation);
            state.last_generation.set(self.tile_generation);
            return Some(shader::Action::request_redraw());
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
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        };

        action
    }

    fn mouse_interaction(
        &self,
        state_cell: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let state = state_cell.borrow();
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
    pub(super) last_generation: std::cell::Cell<u64>,
    dragging: bool,
    fitted: bool,
    last_pos: Option<Point>,
    last_bounds: Option<Size>,
    last_reqs: Option<Vec<(u32, TileRange)>>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera: Camera::new(1.0, 1.0),
            current_mip: 0,
            last_generation: std::cell::Cell::new(0),
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
            last_reqs: None,
        }
    }
}
