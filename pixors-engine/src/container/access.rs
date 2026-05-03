use crate::container::tile::TileCoord;

pub trait PixelIterable {
    fn pixel_count(&self) -> usize;
}

pub trait KernelIterable: PixelIterable {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}

pub trait NeighborhoodIterable {
    fn center(&self) -> &TileCoord;
    fn tiles(&self) -> &[TileCoord];
}

pub trait IndexIterable {
    type Item;
    fn count(&self) -> u32;
    fn get(&self, index: u32) -> Option<&Self::Item>;
}
