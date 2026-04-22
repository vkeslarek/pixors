//! Image types: raw (runtime-resolved) and typed (compile-time pixel type).

mod alpha;
mod channel;
mod sample;
mod raw;
mod typed;

pub use alpha::AlphaMode;
pub use channel::{ChannelKind, ChannelLayoutKind};
pub use sample::{SampleType, SampleLayout};
pub use raw::RawImage;
pub use typed::TypedImage;