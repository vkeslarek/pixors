use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

pub struct BufferPool {
    device: Arc<wgpu::Device>,
    free: Mutex<HashMap<(u64, wgpu::BufferUsages), Vec<Arc<wgpu::Buffer>>>>,
}

impl BufferPool {
    pub fn new(device: Arc<wgpu::Device>) -> Arc<Self> {
        Arc::new(Self {
            device,
            free: Mutex::new(HashMap::new()),
        })
    }

    pub fn acquire(&self, size: u64, usage: wgpu::BufferUsages) -> PooledBuffer {
        let class_size = size_class(size);
        let key = (class_size, usage);
        let mut free_map = self.free.lock().unwrap();
        if let Some(buf) = free_map.get_mut(&key).and_then(|v| v.pop()) {
            return PooledBuffer {
                pool: None,
                buf: Some(buf),
                key,
            };
        }
        drop(free_map);

        let buf = Arc::new(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pooled"),
            size: class_size,
            usage,
            mapped_at_creation: false,
        }));
        PooledBuffer {
            pool: None,
            buf: Some(buf),
            key,
        }
    }

    fn return_buffer(&self, buf: Arc<wgpu::Buffer>, key: (u64, wgpu::BufferUsages)) {
        let mut free_map = self.free.lock().unwrap();
        free_map.entry(key).or_default().push(buf);
    }
}

pub struct PooledBuffer {
    pool: Option<Arc<BufferPool>>,
    buf: Option<Arc<wgpu::Buffer>>,
    key: (u64, wgpu::BufferUsages),
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let (Some(pool), Some(buf)) = (self.pool.take(), self.buf.take()) {
            pool.return_buffer(buf, self.key);
        }
    }
}

impl Deref for PooledBuffer {
    type Target = wgpu::Buffer;

    fn deref(&self) -> &Self::Target {
        self.buf.as_ref().unwrap()
    }
}

impl PooledBuffer {
    pub fn arc(&self) -> Arc<wgpu::Buffer> {
        self.buf.as_ref().unwrap().clone()
    }
}

fn size_class(size: u64) -> u64 {
    let mut n = size.next_power_of_two();
    if n < 256 {
        n = 256;
    }
    n
}
