use crate::util::{read_or_recover, write_or_recover};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

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

const PREFETCH_LOOKAHEAD_S: f32 = 0.25;
const PREFETCH_VEL_THRESHOLD: f32 = 50.0;
const PAN_VEL_EMA_ALPHA: f32 = 0.7;
/// Exponential decay rate for pan inertia (image px/s²). Half-life ≈ 150 ms.
const PAN_DECAY_K: f32 = 4.6;
/// Pan inertia stops when speed drops below this (image px/s).
const PAN_STOP_THRESHOLD: f32 = 5.0;
/// Zoom spring constant: controls how fast zoom settles on target. 99% in ~120 ms.
const ZOOM_SPRING_K: f32 = 38.0;
/// Zoom animation stops when difference is below this.
const ZOOM_STOP_THRESHOLD: f32 = 0.0005;

pub struct ViewportProgram {
    pub cache: Option<Arc<Mutex<TileCache>>>,
    pub redraw_seq: u64,
    pub mip_fetch_queue: Arc<Mutex<Vec<(SessionId, u32, TileRange)>>>,
    pub prefetch_queue: Option<Arc<Mutex<Vec<(SessionId, u32, TileRange)>>>>,
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

        // Keep the redraw loop alive while tiles are pending GPU upload.
        let has_pending = self
            .cache
            .as_ref()
            .and_then(|c| c.lock().ok())
            .is_some_and(|g| g.has_pending());

        let action = match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.ctrl_held = modifiers.contains(keyboard::Modifiers::CTRL);
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(btn)) => {
                let is_pan = *btn == mouse::Button::Middle
                    || (*btn == mouse::Button::Left && state.ctrl_held);
                if is_pan && cursor.position_in(bounds).is_some() {
                    // Kill inertia when grab starts so it doesn't fight the drag.
                    state.pan_velocity = (0.0, 0.0);
                    state.dragging = true;
                    state.pan_button = Some(*btn);
                    state.last_pos = cursor.position_in(bounds).map(|p| (p.x, p.y));
                    state.last_pan_time = Some(Instant::now());
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
                    // Velocity survives release — inertia glide kicks in via draw().
                }
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(curr) = cursor.position_in(bounds) {
                        if let Some((last_x, last_y)) = state.last_pos {
                            let dx = curr.x - last_x;
                            let dy = curr.y - last_y;
                            state.camera.pan(dx, dy);
                            state.user_interacted = true;

                            let now = Instant::now();
                            if let Some(t) = state.last_pan_time {
                                let dt = now.duration_since(t).as_secs_f32().max(1e-4);
                                // Convert screen-space delta to image-space velocity.
                                let vx = -dx / state.camera.zoom / dt;
                                let vy = -dy / state.camera.zoom / dt;
                                let alpha = PAN_VEL_EMA_ALPHA;
                                state.pan_velocity.0 =
                                    alpha * vx + (1.0 - alpha) * state.pan_velocity.0;
                                state.pan_velocity.1 =
                                    alpha * vy + (1.0 - alpha) * state.pan_velocity.1;
                            }
                            state.last_pan_time = Some(now);
                            state.last_interaction_time = Some(now);
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
                    // Accumulate into zoom_target; draw() glides toward it each frame.
                    let min_zoom = state.camera.min_zoom();
                    state.zoom_target = (state.zoom_target * factor).clamp(min_zoom, 64.0);
                    state.zoom_anchor = pos;
                    state.user_interacted = true;
                    state.last_zoom_direction = steps.signum() as i8;
                    state.last_interaction_time = Some(Instant::now());
                    Some(shader::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            _ => None,
        };

        let is_animating = !state.dragging && {
            let (vx, vy) = state.pan_velocity;
            let speed = (vx * vx + vy * vy).sqrt();
            speed > PAN_STOP_THRESHOLD
                || (state.zoom_target - state.camera.zoom).abs() > ZOOM_STOP_THRESHOLD
        };

        if has_pending || is_animating {
            Some(action.unwrap_or_else(shader::Action::request_redraw))
        } else {
            action
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
                state.zoom_target = state.camera.zoom;
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

        // Physics: apply pan inertia and smooth zoom each frame.
        {
            let now = Instant::now();
            let dt = state
                .last_physics_time
                .map(|t| now.duration_since(t).as_secs_f32().min(0.1))
                .unwrap_or(0.0);
            state.last_physics_time = Some(now);

            if dt > 0.0 {
                // Smooth zoom: exponential spring toward zoom_target, anchor-preserving.
                let zoom_diff = state.zoom_target - state.camera.zoom;
                if zoom_diff.abs() > ZOOM_STOP_THRESHOLD {
                    let lerp = 1.0 - (-ZOOM_SPRING_K * dt).exp();
                    let old_zoom = state.camera.zoom;
                    let new_zoom = (old_zoom + zoom_diff * lerp)
                        .clamp(state.camera.min_zoom(), 64.0);
                    let (ax, ay) = state.zoom_anchor;
                    // Keep the anchor point fixed in image space as zoom changes.
                    let a_img_x = ax / old_zoom + state.camera.pan_x;
                    let a_img_y = ay / old_zoom + state.camera.pan_y;
                    state.camera.zoom = new_zoom;
                    state.camera.pan_x = a_img_x - ax / new_zoom;
                    state.camera.pan_y = a_img_y - ay / new_zoom;
                } else {
                    state.camera.zoom = state.zoom_target;
                }

                // Pan inertia: glide after drag release.
                if !state.dragging {
                    let (vx, vy) = state.pan_velocity;
                    let speed = (vx * vx + vy * vy).sqrt();
                    if speed > PAN_STOP_THRESHOLD {
                        state.camera.pan_x += vx * dt;
                        state.camera.pan_y += vy * dt;
                        let decay = (-PAN_DECAY_K * dt).exp();
                        state.pan_velocity = (vx * decay, vy * decay);
                    } else {
                        state.pan_velocity = (0.0, 0.0);
                    }
                }
            }
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

        // Compute predictive prefetch ranges based on pan velocity / zoom direction.
        if let Some(prefetch_q) = &self.prefetch_queue
            && let Some(session_id) = self.session_id
        {
            let mut prefetch_reqs: Vec<(SessionId, u32, TileRange)> = Vec::new();

            let vel = state.pan_velocity;
            let vel_mag = (vel.0 * vel.0 + vel.1 * vel.1).sqrt();
            let recently_interacted = state
                .last_interaction_time
                .is_some_and(|t| t.elapsed().as_secs_f32() < 0.5);

            if vel_mag > PREFETCH_VEL_THRESHOLD && recently_interacted {
                // Predict where the viewport will be in PREFETCH_LOOKAHEAD_S seconds.
                let predicted = state.camera.predicted_tile_range(
                    state.current_mip,
                    TILE_SIZE,
                    2,
                    vel,
                    PREFETCH_LOOKAHEAD_S,
                );
                prefetch_reqs.push((session_id, state.current_mip, predicted));
            }

            if state.last_zoom_direction != 0 && recently_interacted {
                // Prefetch the adjacent mip in the zoom direction.
                let local_max_mip =
                    compute_max_mip(state.camera.img_w as u32, state.camera.img_h as u32);
                let target_mip = if state.last_zoom_direction > 0 {
                    state.current_mip.saturating_sub(1)
                } else {
                    (state.current_mip + 1).min(local_max_mip)
                };
                if target_mip != state.current_mip {
                    prefetch_reqs.push((
                        session_id,
                        target_mip,
                        state.camera.padded_tile_range(target_mip, TILE_SIZE, 4),
                    ));
                }
            }

            if Some(prefetch_reqs.clone()) != state.last_prefetch_reqs {
                if let Ok(mut q) = prefetch_q.lock() {
                    *q = prefetch_reqs.clone();
                }
                state.last_prefetch_reqs = Some(prefetch_reqs);
            }
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
