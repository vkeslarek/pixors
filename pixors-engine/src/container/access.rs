use crate::container::tile::TileCoord;
use crate::container::Container;

pub trait PixelIterable: Container {
    fn pixel_count(&self) -> usize;
}

pub trait KernelIterable: PixelIterable {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}

pub trait NeighborhoodIterable: Container {
    fn center(&self) -> &TileCoord;
    fn tiles(&self) -> &[TileCoord];
}

pub trait IndexIterable {
    type Item;
    fn count(&self) -> u32;
    fn get(&self, index: u32) -> Option<&Self::Item>;
}
