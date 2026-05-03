use std::sync::{Arc, OnceLock};

/// Singleton wgpu device + queue, initialized lazily.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

static CTX: OnceLock<Option<Arc<GpuContext>>> = OnceLock::new();

/// Try to initialize a GPU context. Returns `None` (cached) when no adapter
/// is available — callers should fall back to CPU silently.
pub fn try_init() -> Option<Arc<GpuContext>> {
    CTX.get_or_init(init_inner).clone()
}

/// Cheap check that does not force initialization beyond the first call.
pub fn gpu_available() -> bool {
    try_init().is_some()
}

fn init_inner() -> Option<Arc<GpuContext>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let all: Vec<_> = instance.enumerate_adapters(wgpu::Backends::all()).into_iter().collect();
    tracing::info!("[pixors] gpu: {} adapter(s) enumerated:", all.len());
    for a in &all {
        let i = a.get_info();
        tracing::info!(
            "  - '{}' backend={:?} type={:?} vendor=0x{:x} device=0x{:x}",
            i.name, i.backend, i.device_type, i.vendor, i.device
        );
    }
    // Cascade: DiscreteGpu → HighPerformance hint → LowPower hint → fail.
    let adapter = all
        .into_iter()
        .find(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu)
        .or_else(|| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
        })
        .or_else(|| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
        });
    let adapter = match adapter {
        Some(a) => a,
        None => {
            tracing::info!("[pixors] gpu: no adapter available; falling back to CPU");
            return None;
        }
    };
    let info = adapter.get_info();
    tracing::info!(
        "[pixors] gpu: selected '{}' backend={:?} type={:?}",
        info.name, info.backend, info.device_type
    );
    let res = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("pixors-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ));
    match res {
        Ok((device, queue)) => Some(Arc::new(GpuContext { device, queue })),
        Err(e) => {
            tracing::info!("[pixors] gpu: request_device failed: {e:?}");
            None
        }
    }
}
