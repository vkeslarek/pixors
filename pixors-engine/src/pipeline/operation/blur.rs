use serde::{Deserialize, Serialize};

use crate::container::{Neighborhood, Tile};
use crate::pipeline::operation::Operation;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlurOp {
    pub radius: u32,
}

impl BlurOp {
    pub fn new(radius: u32) -> Self {
        Self { radius }
    }
}

impl Operation for BlurOp {
    type Input = Neighborhood;
    type Output = Tile;

    fn name(&self) -> &'static str {
        "blur"
    }
}
