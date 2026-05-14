use iced::mouse;
use pixors_engine::cache::cache_reader::TileRange;
use std::time::Instant;

use crate::viewport::camera::Camera;
use pixors_document::SessionId;

pub struct ViewportState {
    pub camera: Camera,
    pub current_mip: u32,
    pub last_generation: u64,
    pub dragging: bool,
    pub fitted: bool,
    /// Set true once user manually pans/zooms. Until then, bounds changes
    /// trigger a re-fit (Iced may give wrong bounds on first frame).
    pub user_interacted: bool,
    pub last_pos: Option<(f32, f32)>,
    pub last_bounds: Option<(f32, f32)>,
    pub last_reqs: Option<Vec<(SessionId, u32, TileRange)>>,
    pub last_prefetch_reqs: Option<Vec<(SessionId, u32, TileRange)>>,
    pub ctrl_held: bool,
    pub pan_button: Option<mouse::Button>,
    /// Image-space pan velocity (pixels/second), smoothed with EMA.
    pub pan_velocity: (f32, f32),
    pub last_pan_time: Option<Instant>,
    /// +1 = zoom-in, -1 = zoom-out, 0 = idle.
    pub last_zoom_direction: i8,
    pub last_interaction_time: Option<Instant>,
    /// Target zoom for smooth zoom animation.
    pub zoom_target: f32,
    /// Screen-space anchor point for zoom animation.
    pub zoom_anchor: (f32, f32),
    /// Last time physics was applied in draw(). Used to compute dt.
    pub last_physics_time: Option<Instant>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera: Camera::new(1.0, 1.0),
            current_mip: 0,
            last_generation: 0,
            dragging: false,
            fitted: false,
            user_interacted: false,
            last_pos: None,
            last_bounds: None,
            last_reqs: None,
            last_prefetch_reqs: None,
            ctrl_held: false,
            pan_button: None,
            pan_velocity: (0.0, 0.0),
            last_pan_time: None,
            last_zoom_direction: 0,
            last_interaction_time: None,
            zoom_target: 1.0,
            zoom_anchor: (0.0, 0.0),
            last_physics_time: None,
        }
    }
}
