//! GPU runtime backed by wgpu. The engine is CPU-first; this module is
//! optional and silently disabled when no adapter is available.
//!
//! Kernel definitions, buffer pool, and scheduling moved to `pixors-shader`.

pub mod buffer;
pub mod context;
pub mod kernels;

#[cfg(test)]
mod tests;

pub use buffer::{Buffer, GpuBuffer};
pub use context::{GpuContext, gpu_available, try_init};
