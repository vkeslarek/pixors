use pixors_ops::source::cache_reader::TileRange;

use crate::viewport::camera::Camera;
use pixors_state::TabId;

pub struct ViewportState {
    pub camera: Camera,
    pub current_mip: u32,
    pub last_generation: u64,
    pub dragging: bool,
    pub fitted: bool,
    pub last_pos: Option<(f32, f32)>,
    pub last_bounds: Option<(f32, f32)>,
    pub last_reqs: Option<Vec<(TabId, u32, TileRange)>>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera: Camera::new(1.0, 1.0),
            current_mip: 0,
            last_generation: 0,
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
            last_reqs: None,
        }
    }
}
