//! GPU runtime backed by wgpu. The engine is CPU-first; this module is
//! optional and silently disabled when no adapter is available.

pub mod buffer;
pub mod context;
pub mod kernel;
pub mod kernels;
pub mod pool;
pub mod scheduler;

#[cfg(test)]
mod tests;

pub use buffer::{Buffer, GpuBuffer};
pub use context::{GpuContext, gpu_available, try_init};
