#![allow(dead_code)]
use std::sync::{Arc, OnceLock};

use pixors_engine::debug_stopwatch;
use pixors_engine::error::Error;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
    StageHints,
};

pub struct ViewportTarget {
    pub texture: Arc<wgpu::Texture>,
    pub queue: Arc<wgpu::Queue>,
}

static TARGET: OnceLock<ViewportTarget> = OnceLock::new();

pub fn install_viewport(target: ViewportTarget) {
    let _ = TARGET.set(target);
}

static VS_INPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "tile",
    kind: DataKind::Tile,
}];
static VS_OUTPUTS: &[PortDeclaration] = &[];
static VS_PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Fixed(VS_INPUTS),
    outputs: PortGroup::Fixed(VS_OUTPUTS),
};

#[derive(Debug, Clone)]
pub struct ViewportSink {
    pub width: u32,
    pub height: u32,
}

impl Processor for ViewportSink {
    fn kind(&self) -> &'static str {
        "viewport_sink"
    }
    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &VS_PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints::prefer_gpu()
    }

    fn process(&mut self, _ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let _sw = debug_stopwatch!("viewport_sink");
        let tile = ProcessorContext::take_tile(item)?;

        let gpu_ctx = pixors_engine::gpu::context::try_init()
            .ok_or_else(|| Error::internal("GPU unavailable for viewport"))?;
        gpu_ctx.scheduler().flush_dispatches();

        let target = TARGET
            .get()
            .ok_or_else(|| Error::internal("viewport not installed"))?;

        let (buf, _) = match &tile.data {
            pixors_engine::data::buffer::Buffer::Gpu(g) => (g.buffer(), g.requested_size),
            _ => return Ok(()),
        };

        let tw = tile.coord.width;
        let th = tile.coord.height;
        let px = tile.coord.px;
        let py = tile.coord.py;
        if px + tw > self.width || py + th > self.height {
            return Ok(());
        }

        let bpp = tile.meta.format.bytes_per_pixel() as u32;
        let row_bytes = tw * bpp;
        let padded = row_bytes.div_ceil(256) * 256;
        let mut enc = gpu_ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vpsink_copy"),
            });
        // Source GPU buffer is tightly packed (stride = row_bytes). wgpu requires
        // bytes_per_row to be a multiple of 256 when copying height > 1. When the
        // tight stride is not aligned, pad rows into a staging buffer first.
        let staging_owned = if row_bytes == padded {
            None
        } else {
            let staging = gpu_ctx
                .scheduler()
                .allocate_buffer(padded as u64 * th as u64);
            for row in 0..th {
                enc.copy_buffer_to_buffer(
                    buf,
                    (row * row_bytes) as u64,
                    staging.buffer(),
                    (row * padded) as u64,
                    row_bytes as u64,
                );
            }
            Some(staging)
        };
        let src_for_copy: &wgpu::Buffer = match &staging_owned {
            Some(s) => s.buffer(),
            None => buf,
        };
        enc.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: src_for_copy,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            wgpu::ImageCopyTexture {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: px, y: py, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: 1,
            },
        );
        target.queue.submit(std::iter::once(enc.finish()));
        Ok(())
    }
}
