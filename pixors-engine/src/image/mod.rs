//! Image types: raw (runtime-resolved) and typed (compile-time pixel type).

mod meta;
mod raw;
mod typed;
mod tile;
mod mip;
pub mod buffer;

pub use meta::AlphaMode;
pub use meta::{ChannelKind, ChannelLayoutKind};
pub use meta::{SampleType, SampleLayout};
pub use raw::RawImage;
pub use typed::TypedImage;
pub use tile::{Tile, TileCoord, TileGrid, TileRect};
pub use mip::{MipLevel, MipPyramid, mip_level_for_zoom, generate_from_mip0};
pub use buffer::{ComponentEncoding, PlaneDesc, BufferDesc, ImageBuffer, BandBuffer};
