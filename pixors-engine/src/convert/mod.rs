//! Color and pixel conversion pipelines.

pub mod pipeline;
pub mod tile_stream;
pub mod conversion;
pub mod matrix;

pub use conversion::{ColorConversion, lookup_encode};
pub use matrix::{Matrix3x3, bradford_cat, rgb_to_rgb_transform, rgb_to_xyz_matrix};
