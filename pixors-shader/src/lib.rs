pub mod kernel;
pub mod pool;
pub mod scheduler;

/// Pre-compiled WGSL kernel bodies.
pub mod wgsl {
    pub const BLUR: &str = include_str!("kernels/blur.wgsl");
}
