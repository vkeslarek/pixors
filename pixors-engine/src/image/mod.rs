//! Image types: raw (runtime-resolved) and typed (compile-time pixel type).

mod meta;
mod raw;
mod typed;

pub use meta::AlphaMode;
pub use meta::{ChannelKind, ChannelLayoutKind};
pub use meta::{SampleType, SampleLayout};
pub use raw::RawImage;
pub use typed::TypedImage;