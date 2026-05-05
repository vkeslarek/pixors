//! Image types: raw (runtime-resolved) and typed (compile-time pixel type).

pub mod buffer;
pub mod decoder;
pub mod desc;
pub mod document;
pub mod image;
mod meta;
mod mip;
pub mod neighborhood;
mod tile;

pub use buffer::{BufferDesc, ImageBuffer, PlaneDesc, SampleFormat};
pub use decoder::{ImageDecoder, PageStream};
pub use desc::{BlendMode, Dpi, ImageDesc, Orientation, PageInfo, PixelOffset};
pub use document::{ImageInfo, ImageMetadata, Layer, LayerMetadata};
pub use image::Image;
pub use meta::AlphaMode;
pub use meta::{ChannelKind, ChannelLayoutKind};
pub use meta::{SampleLayout, SampleType};
pub use mip::{MipLevel, MipPyramid};
pub use neighborhood::{EdgeCondition, Neighborhood, NeighborhoodCoord};
pub use tile::{Tile, TileCoord, TileGrid};
