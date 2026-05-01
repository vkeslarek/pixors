#[derive(Debug, Clone, Copy)]
pub struct GpuHandle(pub u32);

pub enum Buffer {
    Cpu(Vec<u8>),
    Gpu(GpuHandle),
}
