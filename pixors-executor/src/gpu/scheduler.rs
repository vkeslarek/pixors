use crate::error::Error;
use crate::gpu::kernel::GpuKernel;
use crate::gpu::pool::{BufferPool, GpuBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::cache::{self, PipelineCache};
use super::encoder::EncoderSlot;

const NUM_SLOTS: usize = 4;
const BATCH_SIZE: usize = 32;

pub struct Scheduler {
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
    pool: Arc<BufferPool>,
    cache: PipelineCache,
    slots: Vec<EncoderSlot>,
    next_slot: AtomicUsize,
}

impl Scheduler {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Arc<Self> {
        let slots = (0..NUM_SLOTS).map(|_| EncoderSlot::new()).collect();
        Arc::new(Self {
            pool: BufferPool::new(device.clone()),
            cache: PipelineCache::new(),
            slots,
            next_slot: AtomicUsize::new(0),
            device,
            queue,
        })
    }

    // ── Slot pinning ──────────────────────────────────────────────────────

    /// Pin each OS thread to one encoder slot so that all dispatches within
    /// a sequential chain (same thread) are submitted in order.
    fn slot_index(&self) -> usize {
        thread_local! {
            static SLOT: std::cell::RefCell<Option<usize>> = const { std::cell::RefCell::new(None) };
        }
        SLOT.with(|s| {
            let mut s = s.borrow_mut();
            *s.get_or_insert_with(|| {
                self.next_slot.fetch_add(1, Ordering::Relaxed) % self.slots.len()
            })
        })
    }

    // ── Dispatch ──────────────────────────────────────────────────────────

    /// Dispatch a compute kernel producing a new output buffer.
    pub fn dispatch_one(
        &self,
        kernel: &dyn GpuKernel,
        inputs: &[&Arc<GpuBuffer>],
        out_buf: GpuBuffer,
        dispatch_x: u32,
        dispatch_y: u32,
    ) -> Result<GpuBuffer, String> {
        let input_bufs: Vec<&wgpu::Buffer> = inputs.iter().map(|b| b.buffer()).collect();
        self.record_dispatch(
            kernel,
            &input_bufs,
            out_buf.buffer(),
            dispatch_x,
            dispatch_y,
        );
        Ok(out_buf)
    }

    /// In-place dispatch: the shader reads from and writes to `buf`.
    pub fn dispatch_inplace(
        &self,
        kernel: &dyn GpuKernel,
        buf: &Arc<GpuBuffer>,
        dispatch_x: u32,
        dispatch_y: u32,
    ) -> Result<(), String> {
        self.record_dispatch(kernel, &[], buf.buffer(), dispatch_x, dispatch_y);
        Ok(())
    }

    /// Shared dispatch logic — prepares pipeline + bind groups, records into
    /// a batched encoder slot, flushes when the batch is full.
    fn record_dispatch(
        &self,
        kernel: &dyn GpuKernel,
        inputs: &[&wgpu::Buffer],
        output: &wgpu::Buffer,
        dispatch_x: u32,
        dispatch_y: u32,
    ) {
        let sig = kernel.signature();
        let sig_hash = cache::hash_signature(sig);
        let cached = self.cache.get_or_build(sig_hash, sig, &self.device);
        let param_buf = cache::upload_params(&self.device, &self.queue, sig, kernel);
        let (bg0, bg1) =
            cache::build_bind_groups(&self.device, &cached, inputs, &param_buf, output);

        let idx = self.slot_index();
        let slot = &self.slots[idx];

        let should_flush = {
            let mut state = slot.lock();
            let encoder = state.encoder(&self.device);

            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("kernel_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&cached.pipeline);
                pass.set_bind_group(0, &bg0, &[]);
                if let Some(ref bg1) = bg1 {
                    pass.set_bind_group(1, bg1, &[]);
                }
                pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
            }

            state.keep_alive.push(param_buf);
            slot.inc_dispatch() >= BATCH_SIZE
        };

        if should_flush {
            self.flush_slot(idx);
        }
    }

    // ── Flush ─────────────────────────────────────────────────────────────

    fn flush_slot(&self, idx: usize) {
        let slot = &self.slots[idx];
        let mut state = slot.lock();
        if let Some(encoder) = state.encoder.take() {
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        state.keep_alive.clear();
        slot.reset_count();
    }

    /// Flush all slots — called by download stages before reading GPU buffers.
    pub fn flush(&self) {
        for idx in 0..self.slots.len() {
            self.flush_slot(idx);
        }
    }

    // ── Buffer ops ────────────────────────────────────────────────────────

    pub fn pool(&self) -> &Arc<BufferPool> {
        &self.pool
    }

    pub fn allocate_buffer(&self, size: u64) -> GpuBuffer {
        self.pool.acquire(
            size,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        )
    }

    /// Deep-copy a GPU buffer by allocating new memory and issuing a copy command.
    pub fn deep_copy_buffer(&self, src: &GpuBuffer) -> Result<GpuBuffer, Error> {
        let dst = self.allocate_buffer(src.requested_size);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("deep_copy"),
            });
        enc.copy_buffer_to_buffer(src.buffer(), 0, dst.buffer(), 0, src.allocated_size);
        self.queue.submit(std::iter::once(enc.finish()));
        Ok(dst)
    }

    /// Upload raw bytes into a GPU buffer.
    pub(crate) fn upload_bytes(&self, data: &[u8]) -> GpuBuffer {
        let original_len = data.len() as u64;
        let aligned_len = (original_len + 3) & !3; // round to COPY_BUFFER_ALIGNMENT (4)
        let mut buf = self.allocate_buffer(aligned_len);
        buf.requested_size = original_len; // download truncates back to original
        self.flush();
        if aligned_len as usize == data.len() {
            self.queue.write_buffer(buf.buffer(), 0, data);
        } else {
            let mut padded = data.to_vec();
            padded.resize(aligned_len as usize, 0);
            self.queue.write_buffer(buf.buffer(), 0, &padded);
        }
        buf
    }

    // ── Public pipeline accessors ─────────────────────────────────────────

    pub fn compute_pipeline(
        &self,
        sig: &crate::gpu::kernel::KernelSignature,
    ) -> Result<Arc<wgpu::ComputePipeline>, String> {
        let sig_hash = cache::hash_signature(sig);
        let cached = self.cache.get_or_build(sig_hash, sig, &self.device);
        Ok(cached.pipeline)
    }

    pub fn bind_group_layout(
        &self,
        sig: &crate::gpu::kernel::KernelSignature,
    ) -> Result<Arc<wgpu::BindGroupLayout>, String> {
        let sig_hash = cache::hash_signature(sig);
        let cached = self.cache.get_or_build(sig_hash, sig, &self.device);
        cached
            .bgls
            .first()
            .cloned()
            .ok_or_else(|| "no BGL".to_string())
    }
}
