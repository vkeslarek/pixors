pub mod meta;
pub mod tile;
pub mod scanline;
pub mod neighborhood;
pub mod layer;
pub mod image;
pub mod access;

use crate::storage::Buffer;

pub use access::{IndexIterable, KernelIterable, NeighborhoodIterable, PixelIterable};
pub use meta::PixelMeta;
pub use tile::{Tile, TileCoord};
pub use scanline::{ScanLine, ScanLineCoord};
pub use neighborhood::{EdgeCondition, Neighborhood, NeighborhoodCoord};
pub use layer::{Layer, Layers};
pub use image::Image;

pub trait Container {
    fn meta(&self) -> &PixelMeta;
}

pub struct ContainerInstance<C: Container> {
    pub container: C,
    pub data: Buffer,
}

impl<C: Container> ContainerInstance<C> {
    pub fn new(container: C, data: Buffer) -> Self {
        Self { container, data }
    }

    pub fn meta(&self) -> &PixelMeta {
        self.container.meta()
    }
}
