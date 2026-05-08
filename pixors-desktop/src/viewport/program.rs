use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use iced::widget::shader;
use iced::{Event, Point, Rectangle, Size};
use iced::mouse;
use pixors_executor::source::cache_reader::TileRange;

use crate::state::TabId;
use crate::viewport::camera::Camera;
use crate::viewport::pipeline::ViewportPrimitive;
use crate::viewport::state::ViewportState;
use crate::viewport::tile_cache::ViewportCache;

pub const TILE_SIZE: u32 = 256;

pub struct ViewportProgram {
    pub cache: Option<Arc<Mutex<ViewportCache>>>,
    pub tile_generation: u64,
    pub mip_fetch_signal: Arc<Mutex<Vec<(TabId, u32, TileRange)>>>,
    pub tab_id: Option<TabId>,
    pub viewport_state: Option<Rc<RefCell<ViewportState>>>,
}

impl<Msg> shader::Program<Msg> for ViewportProgram {
    type State = ();
    type Primitive = ViewportPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let Some(ref vp_state) = self.viewport_state else {
            return ViewportPrimitive {
                camera: Camera::new(1.0, 1.0).to_uniform(0),
                cache: None,
                visible_range: TileRange { tx_start: 0, tx_end: 0, ty_start: 0, ty_end: 0 },
            };
        };
        let mut state = vp_state.borrow_mut();

        let old_mip = state.current_mip;

        if let Some(ref cache) = self.cache
            && let Ok(mut guard) = cache.lock()
                && let Some((img_w, img_h)) = guard.take_new_img() {
                    state.camera.img_w = img_w as f32;
                    state.camera.img_h = img_h as f32;
                    state.camera.fit();
                    state.current_mip = state.camera.visible_mip_level();
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

        if let Some(ref cache) = self.cache
            && let Ok(guard) = cache.lock() {
                let base_mip = state.camera.floor_mip();
                if target_mip > base_mip && !guard.has_mip(target_mip) && guard.has_mip(base_mip) {
                    tracing::info!("[pixors] viewport: fallback from target {} to mip {}", target_mip, base_mip);
                    target_mip = base_mip;
                }
            }

        if state.current_mip != target_mip {
            tracing::info!("[pixors] viewport: draw() setting current_mip to {}", target_mip);
        }
        state.current_mip = target_mip;

        let mut reqs = Vec::new();
        if let Some(tab_id) = self.tab_id {
            reqs.push((tab_id, state.current_mip, state.camera.padded_tile_range(state.current_mip, TILE_SIZE, 3)));

            let max_mip = crate::viewport::camera::compute_max_mip(state.camera.img_w as u32, state.camera.img_h as u32);
            if state.current_mip < max_mip {
                reqs.push((tab_id, state.current_mip + 1, state.camera.padded_tile_range(state.current_mip + 1, TILE_SIZE, 2)));
            }
            if state.current_mip > 0 {
                reqs.push((tab_id, state.current_mip - 1, state.camera.padded_tile_range(state.current_mip - 1, TILE_SIZE, 2)));
            }
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
        _state_cell: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        let Some(ref vp_state) = self.viewport_state else {
            return None;
        };
        let mut state = vp_state.borrow_mut();

        if self.tile_generation != state.last_generation.get() {
            tracing::info!("[pixors] viewport: update() saw generation change ({} -> {}), requesting redraw", state.last_generation.get(), self.tile_generation);
            state.last_generation.set(self.tile_generation);
            return Some(shader::Action::request_redraw());
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
        }
    }

    fn mouse_interaction(
        &self,
        _state_cell: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(ref vp_state) = self.viewport_state else {
            return mouse::Interaction::default();
        };
        let state = vp_state.borrow();
        if state.dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::default()
        }
    }
}
