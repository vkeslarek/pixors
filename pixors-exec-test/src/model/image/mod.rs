//! Image types: raw (runtime-resolved) and typed (compile-time pixel type).

mod meta;
mod tile;
mod mip;
pub mod buffer;
pub mod document;
pub mod neighborhood;

pub use meta::AlphaMode;
pub use meta::{ChannelKind, ChannelLayoutKind};
pub use meta::{SampleType, SampleLayout};
pub use tile::{Tile, TileCoord, TileGrid};
pub use mip::{MipLevel, MipPyramid};
pub use buffer::{SampleFormat, PlaneDesc, BufferDesc, ImageBuffer};
pub use document::{Image, ImageMetadata, ImageInfo, Layer, LayerMetadata, Orientation, BlendMode};
pub use neighborhood::{EdgeCondition, Neighborhood, NeighborhoodCoord};
