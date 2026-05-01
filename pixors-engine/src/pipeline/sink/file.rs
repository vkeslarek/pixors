use serde::{Deserialize, Serialize};

use crate::container::Tile;
use crate::pipeline::sink::Sink;

#[derive(Clone, Serialize, Deserialize)]
pub struct ImageFileSink {
    pub path: std::path::PathBuf,
}

impl ImageFileSink {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Sink for ImageFileSink {
    type Input = Tile;

    fn name(&self) -> &'static str {
        "image_file_sink"
    }
}
