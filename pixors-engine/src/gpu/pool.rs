use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_BUF_ID: AtomicU64 = AtomicU64::new(1);

fn next_buf_id() -> u64 {
    NEXT_BUF_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct BufferPool {
    device: Arc<wgpu::Device>,
    free: DashMap<(u64, wgpu::BufferUsages), SegQueue<wgpu::Buffer>>,
    pending: SegQueue<(wgpu::Buffer, u64, wgpu::BufferUsages)>,
}

impl BufferPool {
    pub fn new(device: Arc<wgpu::Device>) -> Arc<Self> {
        Arc::new(Self {
            device,
            free: DashMap::new(),
            pending: SegQueue::new(),
        })
    }

    pub fn acquire(self: &Arc<Self>, size: u64, usage: wgpu::BufferUsages) -> GpuBuffer {
        let class_size = size_class(size);
        let key = (class_size, usage);

        let queue = self.free.entry(key).or_default();

        if let Some(buf) = queue.pop() {
            let id = next_buf_id();
            tracing::info!(
                "[buf] #{id} acquire RECYCLED class={class_size} req={size} usage={usage:?}",
            );
            return GpuBuffer {
                id,
                allocated_size: class_size,
                requested_size: size,
                usage,
                pool: self.clone(),
                buffer: Some(buf),
            };
        }

        let id = next_buf_id();
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("buf-{id}")),
            size: class_size,
            usage,
            mapped_at_creation: false,
        });

        tracing::info!("[buf] #{id} acquire NEW class={class_size} req={size} usage={usage:?}",);

        GpuBuffer {
            id,
            allocated_size: class_size,
            requested_size: size,
            usage,
            pool: self.clone(),
            buffer: Some(buf),
        }
    }

    fn return_buffer_free(&self, buf: wgpu::Buffer, size: u64, usage: wgpu::BufferUsages) {
        let key = (size, usage);
        self.free.entry(key).or_default().push(buf);
    }

    /// Only recycle pending buffers when GPU work is known to be done
    /// (caller must have flushed/submitted+waited).
    pub fn recycle_pending(&self) {
        while let Some((buf, size, usage)) = self.pending.pop() {
            self.return_buffer_free(buf, size, usage);
        }
    }
}

pub struct GpuBuffer {
    pub id: u64,
    pub allocated_size: u64,
    pub requested_size: u64,
    pub usage: wgpu::BufferUsages,
    pub pool: Arc<BufferPool>,
    pub buffer: Option<wgpu::Buffer>,
}

impl std::fmt::Debug for GpuBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuBuffer")
            .field("id", &self.id)
            .field("allocated_size", &self.allocated_size)
            .field("requested_size", &self.requested_size)
            .field("usage", &self.usage)
            .finish()
    }
}

impl GpuBuffer {
    pub fn buffer(&self) -> &wgpu::Buffer {
        self.buffer.as_ref().unwrap()
    }
}

impl Drop for GpuBuffer {
    fn drop(&mut self) {
        if let Some(buf) = self.buffer.take() {
            self.pool
                .pending
                .push((buf, self.allocated_size, self.usage));
        }
    }
}

fn size_class(size: u64) -> u64 {
    let mut n = size.next_power_of_two();
    if n < 256 {
        n = 256;
    }
    n
}
