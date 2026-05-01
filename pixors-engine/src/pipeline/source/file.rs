use serde::{Deserialize, Serialize};

use crate::container::Tile;
use crate::pipeline::source::Source;

#[derive(Clone, Serialize, Deserialize)]
pub struct FileImageSource {
    pub path: std::path::PathBuf,
}

impl FileImageSource {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
        }
    }
}

impl Source for FileImageSource {
    type Output = Tile;

    fn name(&self) -> &'static str {
        "file_image"
    }
}
