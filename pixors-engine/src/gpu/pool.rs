use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use std::sync::Arc;

pub struct BufferPool {
    device: Arc<wgpu::Device>,
    free: DashMap<(u64, wgpu::BufferUsages), SegQueue<wgpu::Buffer>>,
}

impl BufferPool {
    pub fn new(device: Arc<wgpu::Device>) -> Arc<Self> {
        Arc::new(Self {
            device,
            free: DashMap::new(),
        })
    }

    pub fn acquire(self: &Arc<Self>, size: u64, usage: wgpu::BufferUsages) -> GpuBuffer {
        let class_size = size_class(size);
        let key = (class_size, usage);

        let queue = self.free.entry(key).or_default();

        if let Some(buf) = queue.pop() {
            return GpuBuffer {
                allocated_size: class_size,
                requested_size: size,
                usage,
                pool: self.clone(),
                buffer: Some(buf),
            };
        }

        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pooled"),
            size: class_size,
            usage,
            mapped_at_creation: false,
        });

        GpuBuffer {
            allocated_size: class_size,
            requested_size: size,
            usage,
            pool: self.clone(),
            buffer: Some(buf),
        }
    }

    pub fn return_buffer(&self, buf: wgpu::Buffer, size: u64, usage: wgpu::BufferUsages) {
        let key = (size, usage);
        self.free.entry(key).or_default().push(buf);
    }
}

pub struct GpuBuffer {
    pub allocated_size: u64,
    pub requested_size: u64,
    pub usage: wgpu::BufferUsages,
    pub pool: Arc<BufferPool>,
    pub buffer: Option<wgpu::Buffer>,
}

impl std::fmt::Debug for GpuBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuBuffer")
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
                .return_buffer(buf, self.allocated_size, self.usage);
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
