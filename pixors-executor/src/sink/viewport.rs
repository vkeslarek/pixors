use std::sync::{Arc, OnceLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::stage::{BufferAccess, CpuKernel, DataKind, PortDecl, PortSpec, Stage, StageHints};
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::error::Error;
use crate::debug_stopwatch;

// ── Global viewport (installed by desktop, consumed by sink) ──────────────────

pub struct ViewportTarget {
    pub texture: Arc<wgpu::Texture>,
    pub queue: Arc<wgpu::Queue>,
}

static TARGET: OnceLock<ViewportTarget> = OnceLock::new();

pub fn install_viewport(target: ViewportTarget) {
    let _ = TARGET.set(target);
}

// ── Stage ───────────────────────────────────────────────────────────────────

static VS_INPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static VS_OUTPUTS: &[PortDecl] = &[];
static VS_PORTS: PortSpec = PortSpec { inputs: VS_INPUTS, outputs: VS_OUTPUTS };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportSink {
    pub width: u32,
    pub height: u32,
}

impl Stage for ViewportSink {
    fn kind(&self) -> &'static str { "viewport_sink" }
    fn ports(&self) -> &'static PortSpec { &VS_PORTS }
    fn hints(&self) -> StageHints {
        StageHints { buffer_access: BufferAccess::ReadOnly, prefers_gpu: false }
    }
    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(ViewportSinkRunner { width: self.width, height: self.height }))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────────

pub struct ViewportSinkRunner {
    width: u32,
    height: u32,
}

impl CpuKernel for ViewportSinkRunner {
    fn process(&mut self, item: Item, _emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("viewport_sink");
        let tile = match item { Item::Tile(t) => t, _ => return Ok(()), };

        let target = TARGET.get().ok_or_else(|| Error::internal("viewport not installed"))?;

        let (buf, _) = match &tile.data {
            crate::data::Buffer::Gpu(g) => (&g.buffer, g.size),
            _ => return Ok(()),
        };

        let tw = tile.coord.width;
        let th = tile.coord.height;
        let px = tile.coord.px;
        let py = tile.coord.py;
        if px + tw > self.width || py + th > self.height { return Ok(()); }

        let padded = ((tw * 4 + 255) / 256) * 256;
        let ctx = crate::gpu::try_init().ok_or_else(|| Error::internal("GPU unavailable"))?;
        let mut enc = ctx.device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("vpsink_copy") });
        enc.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: buf,
                layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: None },
            },
            wgpu::ImageCopyTexture {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: px, y: py, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
        );
        target.queue.submit(std::iter::once(enc.finish()));
        Ok(())
    }
}
