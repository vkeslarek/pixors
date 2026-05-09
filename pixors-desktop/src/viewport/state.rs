use std::cell::Cell;

use iced::{Point, Size};
use pixors_ops::source::cache_reader::TileRange;

use crate::state::TabId;
use crate::viewport::camera::Camera;

pub struct ViewportState {
    pub camera: Camera,
    pub current_mip: u32,
    pub last_generation: Cell<u64>,
    pub dragging: bool,
    pub fitted: bool,
    pub last_pos: Option<Point>,
    pub last_bounds: Option<Size>,
    pub last_reqs: Option<Vec<(TabId, u32, TileRange)>>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            camera: Camera::new(1.0, 1.0),
            current_mip: 0,
            last_generation: Cell::new(0),
            dragging: false,
            fitted: false,
            last_pos: None,
            last_bounds: None,
            last_reqs: None,
        }
    }
}
