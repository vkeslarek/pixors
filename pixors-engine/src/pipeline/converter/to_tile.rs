use serde::{Deserialize, Serialize};

use crate::container::{ScanLine, Tile};
use crate::pipeline::converter::Converter;

#[derive(Clone, Serialize, Deserialize)]
pub struct ScanLineToTile {
    pub tile_size: u32,
    pub image_width: u32,
}

impl ScanLineToTile {
    pub fn new(tile_size: u32, image_width: u32) -> Self {
        Self {
            tile_size,
            image_width,
        }
    }
}

impl Converter for ScanLineToTile {
    type Input = ScanLine;
    type Output = Tile;

    fn name(&self) -> &'static str {
        "scanline_to_tile"
    }
}
