use crate::util::{read_or_recover, write_or_recover};
use std::sync::{Arc, Mutex, RwLock};

use iced::keyboard;
use iced::mouse;
use iced::widget::shader;
use iced::{Event, Rectangle};
use pixors_engine::cache::cache_reader::TileRange;

use crate::viewport::camera::{Camera, compute_max_mip};
use crate::viewport::pipeline::ViewportPrimitive;
use crate::viewport::tile_cache::TileCache;
use crate::viewport::viewport_state::ViewportState;
use pixors_document::SessionId;
use pixors_document::TILE_SIZE;

pub struct ViewportProgram {
    pub cache: Option<Arc<Mutex<TileCache>>>,
    pub redraw_seq: u64,
    pub mip_fetch_queue: Arc<Mutex<Vec<(SessionId, u32, TileRange)>>>,
    pub session_id: Option<SessionId>,
    pub viewport_state: Option<Arc<RwLock<ViewportState>>>,
}

impl<Msg> shader::Program<Msg> for ViewportProgram {
    type State = ();
    type Primitive = ViewportPrimitive;

    fn update(
        &self,
        _state_cell: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<Msg>> {
        let vp_state = self.viewport_state.as_ref()?;
        let mut state = write_or_recover(vp_state);

        if self.redraw_seq != state.last_generation {
            tracing::debug!(
                "[pixors] viewport: update() saw generation change ({} -> {}), requesting redraw",
                state.last_generation,
                self.redraw_seq
            );
            state.last_generation = self.redraw_seq;
            return Some(shader::Action::request_redraw());
        }

        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.ctrl_held = modifiers.contains(keyboard::Modifiers::CTRL);
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(btn)) => {
                let is_pan = *btn == mouse::Button::Middle
                    || (*btn == mouse::Button::Left && state.ctrl_held);
                if is_pan && cursor.position_in(bounds).is_some() {
                    state.dragging = true;
                    state.pan_button = Some(*btn);
                    state.last_pos = cursor.position_in(bounds).map(|p| (p.x, p.y));
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(btn)) => {
                if state.pan_button == Some(*btn) {
                    state.dragging = false;
                    state.pan_button = None;
                    state.last_pos = None;
                }
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(curr) = cursor.position_in(bounds) {
                        if let Some((last_x, last_y)) = state.last_pos {
                            state.camera.pan(curr.x - last_x, curr.y - last_y);
                            state.user_interacted = true;
                        }
                        state.last_pos = Some((curr.x, curr.y));
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
                    let pos = cursor
                        .position_in(bounds)
                        .map(|p| (p.x, p.y))
                        .unwrap_or((0.0, 0.0));
                    state.camera.zoom_at(factor, pos.0, pos.1);
                    state.user_interacted = true;
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

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
                visible_range: TileRange {
                    tx_start: 0,
                    tx_end: 0,
                    ty_start: 0,
                    ty_end: 0,
                },
            };
        };
        let mut state = write_or_recover(vp_state);

        let old_mip = state.current_mip;

        let bounds_tuple = (bounds.width, bounds.height);
        if state.last_bounds != Some(bounds_tuple) {
            state.camera.resize(bounds_tuple.0, bounds_tuple.1);
            // Re-fit on every bounds change until user manually interacts.
            // Iced can report incorrect bounds on the first frame; only after
            // pan/zoom should we consider the camera state user-owned.
            if !state.user_interacted {
                state.camera.fit();
                state.fitted = true;
                state.current_mip = state.camera.visible_mip_level();
            }
            state.last_bounds = Some(bounds_tuple);
            tracing::debug!(
                "[viewport] bounds change: bounds={}x{} img={}x{} zoom={:.4} pan=({:.1},{:.1}) mip={} interacted={}",
                bounds_tuple.0,
                bounds_tuple.1,
                state.camera.img_w,
                state.camera.img_h,
                state.camera.zoom,
                state.camera.pan_x,
                state.camera.pan_y,
                state.current_mip,
                state.user_interacted,
            );
        }

        let mut target_mip = state.camera.visible_mip_level();

        if let Some(ref cache) = self.cache
            && let Ok(guard) = cache.lock()
        {
            let base_mip = state.camera.floor_mip();
            if target_mip > base_mip && !guard.has_mip(target_mip) && guard.has_mip(base_mip) {
                target_mip = base_mip;
            }
        }

        if state.current_mip != target_mip {
            tracing::debug!(
                "[pixors] viewport: draw() setting current_mip to {}",
                target_mip
            );
        }
        state.current_mip = target_mip;

        let mut reqs = Vec::new();
        if let Some(session_id) = self.session_id {
            reqs.push((
                session_id,
                state.current_mip,
                state
                    .camera
                    .padded_tile_range(state.current_mip, TILE_SIZE, 3),
            ));

            let max_mip = compute_max_mip(state.camera.img_w as u32, state.camera.img_h as u32);
            if state.current_mip < max_mip {
                reqs.push((
                    session_id,
                    state.current_mip + 1,
                    state
                        .camera
                        .padded_tile_range(state.current_mip + 1, TILE_SIZE, 2),
                ));
            }
            if state.current_mip > 0 {
                reqs.push((
                    session_id,
                    state.current_mip - 1,
                    state
                        .camera
                        .padded_tile_range(state.current_mip - 1, TILE_SIZE, 2),
                ));
            }
        }

        if Some(reqs.clone()) != state.last_reqs {
            if state.current_mip != old_mip {
                tracing::debug!(
                    "[pixors] viewport: MIP changed {} → {}",
                    old_mip,
                    state.current_mip,
                );
            }

            if let Ok(mut sig) = self.mip_fetch_queue.lock() {
                *sig = reqs.clone();
            }
            state.last_reqs = Some(reqs);
        }

        ViewportPrimitive {
            camera: state.camera.to_uniform(state.current_mip),
            cache: self.cache.clone(),
            visible_range: state
                .camera
                .padded_tile_range(state.current_mip, TILE_SIZE, 3),
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
        let state = read_or_recover(vp_state);
        if state.dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::default()
        }
    }
}
