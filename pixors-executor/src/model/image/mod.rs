//! Image types: format-agnostic model + I/O.

pub mod buffer;
pub mod decoder;
pub mod desc;
pub mod image;
pub mod layer;
pub mod meta;

pub use buffer::{BufferDesc, ImageBuffer, PlaneDesc, SampleFormat};
pub use decoder::{ImageDecoder, PageStream};
pub use desc::{BlendMode, Dpi, ImageDesc, Orientation, PageInfo, PixelOffset};
pub use image::Image;
pub use layer::Layer;
