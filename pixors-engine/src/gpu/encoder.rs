use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::gpu::pool::GpuBuffer;

pub struct EncoderSlot {
    state: Mutex<SlotState>,
    dispatch_count: AtomicUsize,
}

pub struct SlotState {
    pub encoder: Option<wgpu::CommandEncoder>,
    /// Raw wgpu::Buffers kept alive for the duration of the encoder (param buffers).
    pub keep_alive: Vec<wgpu::Buffer>,
    /// Arc<GpuBuffer> for every input buffer recorded into this encoder slot.
    /// Held until after queue.submit() so the pool cannot recycle them before
    /// the GPU finishes reading them.
    pub keep_alive_gpu: Vec<Arc<GpuBuffer>>,
}

impl EncoderSlot {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SlotState {
                encoder: None,
                keep_alive: Vec::new(),
                keep_alive_gpu: Vec::new(),
            }),
            dispatch_count: AtomicUsize::new(0),
        }
    }

    /// Lock the slot state, returning a guard. Caller records dispatches,
    /// pushes keep-alive buffers, and checks whether batch size was hit.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, SlotState> {
        self.state.lock().unwrap()
    }

    /// Increment the dispatch counter; returns the new count.
    pub fn inc_dispatch(&self) -> usize {
        self.dispatch_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Reset the dispatch counter after a flush.
    pub fn reset_count(&self) {
        self.dispatch_count.store(0, Ordering::Relaxed);
    }
}

impl SlotState {
    /// Ensure an encoder exists (creating one if needed).
    pub fn encoder(&mut self, device: &wgpu::Device) -> &mut wgpu::CommandEncoder {
        self.encoder.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("batch_slot"),
            })
        })
    }
}
