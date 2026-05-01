use serde::{Deserialize, Serialize};

use crate::container::{Neighborhood, Tile};
use crate::pipeline::converter::Converter;

#[derive(Clone, Serialize, Deserialize)]
pub struct TileToNeighborhood {
    pub radius: u32,
}

impl TileToNeighborhood {
    pub fn new(radius: u32) -> Self {
        Self { radius }
    }
}

impl Converter for TileToNeighborhood {
    type Input = Tile;
    type Output = Neighborhood;

    fn name(&self) -> &'static str {
        "tile_to_neighborhood"
    }
}
