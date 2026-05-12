use iced::mouse;
use pixors_ops::source::cache_reader::TileRange;

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
    pub ctrl_held: bool,
    pub pan_button: Option<mouse::Button>,
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
            ctrl_held: false,
            pan_button: None,
        }
    }
}
