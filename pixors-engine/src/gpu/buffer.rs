use std::sync::Arc;

/// Reference-counted handle to a wgpu storage buffer holding pixel data.
#[derive(Clone)]
pub struct GpuBuffer {
    pub buffer: Arc<wgpu::Buffer>,
    pub size: u64,
}

impl GpuBuffer {
    pub fn new(buffer: Arc<wgpu::Buffer>, size: u64) -> Self {
        Self { buffer, size }
    }
}

impl std::fmt::Debug for GpuBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuBuffer").field("size", &self.size).finish()
    }
}
