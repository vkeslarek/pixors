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
        self.record_dispatch(kernel, inputs, out_buf.buffer(), dispatch_x, dispatch_y);
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
        self.record_dispatch(kernel, &[buf], buf.buffer(), dispatch_x, dispatch_y);
        Ok(())
    }

    /// Shared dispatch logic — prepares pipeline + bind groups, records into
    /// a batched encoder slot, flushes when the batch is full.
    /// Input Arcs are retained in the slot's keep_alive_gpu until after submit.
    fn record_dispatch(
        &self,
        kernel: &dyn GpuKernel,
        inputs: &[&Arc<GpuBuffer>],
        output: &wgpu::Buffer,
        dispatch_x: u32,
        dispatch_y: u32,
    ) {
        let sig = kernel.signature();
        let sig_hash = cache::hash_signature(sig);
        let cached = self.cache.get_or_build(sig_hash, sig, &self.device);
        let param_buf = cache::upload_params(&self.device, &self.queue, sig, kernel);
        let input_bufs: Vec<&wgpu::Buffer> = inputs.iter().map(|b| b.buffer()).collect();
        let (bg0, bg1) =
            cache::build_bind_groups(&self.device, &cached, &input_bufs, &param_buf, output);

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
            for arc in inputs {
                state.keep_alive_gpu.push(Arc::clone(arc));
            }
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
        // Drop param buffers and input Arc refs only after submit so the GPU
        // cannot finish and re-queue their memory before the encoder is done.
        state.keep_alive.clear();
        state.keep_alive_gpu.clear();
        slot.reset_count();
    }

    /// Flush all slots — called by download stages before reading GPU buffers.
    /// Polls until all submitted work is complete before recycling pending
    /// pool buffers, preventing use-after-free on the GPU side.
    pub fn flush(&self) {
        for idx in 0..self.slots.len() {
            self.flush_slot(idx);
        }
        self.device.poll(wgpu::Maintain::Wait);
        self.pool.recycle_pending();
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
    pub fn upload_bytes(&self, data: &[u8]) -> GpuBuffer {
        let original_len = data.len() as u64;
        let aligned_len = (original_len + 3) & !3; // round to COPY_BUFFER_ALIGNMENT (4)
        let mut buf = self.allocate_buffer(aligned_len);
        buf.requested_size = original_len; // download truncates back to original
        if aligned_len as usize == data.len() {
            self.queue.write_buffer(buf.buffer(), 0, data);
        } else {
            let mut padded = data.to_vec();
            padded.resize(aligned_len as usize, 0);
            self.queue.write_buffer(buf.buffer(), 0, &padded);
        }
        buf
    }

    /// Allocate a GPU buffer and zero-fill it via a GPU clear command (no CPU round-trip).
    ///
    /// The clear is submitted immediately via its own encoder. Recording it into a
    /// batched slot is unsafe because callers like `copy_tiles_into_padded` and
    /// `dispatch_one` submit via different paths (queue.submit vs slot flush);
    /// without ordering the clear could fire AFTER subsequent writes, zeroing
    /// the buffer back out.
    pub fn alloc_zeroed_buffer(&self, size: u64) -> GpuBuffer {
        let aligned = (size + 3) & !3;
        let buf = self.allocate_buffer(aligned);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("alloc_zeroed"),
            });
        enc.clear_buffer(buf.buffer(), 0, None);
        self.queue.submit(std::iter::once(enc.finish()));
        buf
    }

    /// Copy a slice from one GPU buffer to another.
    pub fn copy_slice(
        &self,
        src: &wgpu::Buffer,
        src_offset: u64,
        dst: &wgpu::Buffer,
        dst_offset: u64,
        size: u64,
    ) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy_slice"),
            });
        enc.copy_buffer_to_buffer(src, src_offset, dst, dst_offset, size);
        self.queue.submit(std::iter::once(enc.finish()));
    }

    /// Copy tile data from a consolidated buffer into a padded buffer,
    /// handling per-row placement relative to the padded origin.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_tiles_into_padded(
        &self,
        src: &wgpu::Buffer,
        tile_infos: &[crate::data::neighborhood::TileGpuInfo],
        dst: &wgpu::Buffer,
        pad_w: usize,
        pad_h: usize,
        orig_x: i64,
        orig_y: i64,
        bpp: usize,
    ) {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy_tiles_padded"),
            });
        let mut total_copied = 0usize;
        for info in tile_infos {
            let tile_w = info.width as usize;
            let tile_h = info.height as usize;
            for row in 0..tile_h {
                let buf_y = info.py as i64 + row as i64 - orig_y;
                if buf_y < 0 || buf_y as usize >= pad_h {
                    continue;
                }
                let buf_x_base = info.px as i64 - orig_x;
                let src_start: usize = if buf_x_base < 0 {
                    (-buf_x_base) as usize
                } else {
                    0
                };
                let dst_start = buf_x_base.max(0) as usize;
                let copy_w = tile_w
                    .saturating_sub(src_start)
                    .min(pad_w.saturating_sub(dst_start));
                if copy_w == 0 {
                    continue;
                }
                let src_off = info.data_offset
                    + (row as u64 * tile_w as u64 * bpp as u64)
                    + (src_start as u64 * bpp as u64);
                let dst_off = ((buf_y as usize * pad_w + dst_start) * bpp) as u64;
                let len = (copy_w * bpp) as u64;
                enc.copy_buffer_to_buffer(src, src_off, dst, dst_off, len);
                total_copied += copy_w;
            }
        }
        tracing::debug!(
            "[copy_padded] {} tiles, {} total pixel-cols copied, pad={pad_w}×{pad_h} orig=({orig_x},{orig_y})",
            tile_infos.len(),
            total_copied,
        );
        self.queue.submit(std::iter::once(enc.finish()));
    }

    /// Read a slice of bytes from a GPU buffer at the given offset.
    pub fn read_from_buffer(&self, src: &wgpu::Buffer, offset: u64, size: u64) -> Vec<u8> {
        let size_aligned = (size + 3) & !3;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read-staging"),
            size: size_aligned,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("read_from_buffer"),
            });
        enc.copy_buffer_to_buffer(src, offset, &staging, 0, size_aligned);
        self.queue.submit(std::iter::once(enc.finish()));
        self.device.poll(wgpu::Maintain::Wait);
        self.pool.recycle_pending();

        let (tx, rx) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("GPU buffer map channel closed unexpectedly")
            .expect("GPU buffer map failed");

        let mut bytes = {
            let view = staging.slice(..).get_mapped_range();
            view.to_vec()
        };
        bytes.truncate(size as usize);
        staging.unmap();
        bytes
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
