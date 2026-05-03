pub mod context;
pub mod kernel;
pub mod pool;
pub mod scheduler;

#[cfg(test)]
mod tests;

pub use context::{GpuContext, gpu_available, try_init};
