use std::sync::Arc;

pub use crate::gpu::buffer::GpuBuffer;

/// Pixel storage. `Cpu` uses an `Arc<Vec<u8>>` for cheap clones (copy-on-write
/// when callers need exclusive access via `Arc::make_mut`). `Gpu` wraps a
/// reference-counted handle to a wgpu storage buffer.
#[derive(Debug, Clone)]
pub enum Buffer {
    Cpu(Arc<Vec<u8>>),
    Gpu(GpuBuffer),
}

impl Buffer {
    pub fn cpu(v: Vec<u8>) -> Self {
        Self::Cpu(Arc::new(v))
    }

    pub fn as_cpu_slice(&self) -> Option<&[u8]> {
        match self {
            Self::Cpu(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn as_gpu(&self) -> Option<&GpuBuffer> {
        match self {
            Self::Gpu(g) => Some(g),
            _ => None,
        }
    }

    pub fn is_cpu(&self) -> bool {
        matches!(self, Self::Cpu(_))
    }

    pub fn is_gpu(&self) -> bool {
        matches!(self, Self::Gpu(_))
    }
}
