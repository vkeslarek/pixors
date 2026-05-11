use std::collections::HashMap;
use std::sync::Arc;

use pixors_engine::data::buffer::Buffer;
use pixors_engine::data::device::Device;
use pixors_engine::data::tile::{Tile, TileGridPos};
use pixors_engine::error::Error;
use pixors_engine::gpu::context::GpuContext;
use pixors_engine::gpu::pool::GpuBuffer;
use pixors_engine::graph::emitter::Emitter;
use pixors_engine::graph::item::Item;
use pixors_engine::stage::{
    DataKind, InOutPortSpecification, PortDeclaration, PortGroup, Processor, ProcessorContext,
    StageHints,
};
use pixors_image::image::BlendMode;

static COMPOSE_INPUT: PortDeclaration = PortDeclaration {
    name: "layers",
    kind: DataKind::Tile,
};
static COMPOSE_OUTPUTS: &[PortDeclaration] = &[PortDeclaration {
    name: "composed",
    kind: DataKind::Tile,
}];
static COMPOSE_PORTS: InOutPortSpecification = InOutPortSpecification {
    inputs: PortGroup::Variable(&COMPOSE_INPUT),
    outputs: PortGroup::Fixed(COMPOSE_OUTPUTS),
};

#[derive(Debug, Clone)]
pub struct Compose {
    pub layer_count: u16,
    pub blend_modes: Vec<BlendMode>,
    pub opacities: Vec<f32>,
    grid: HashMap<TileGridPos, Vec<Option<Tile>>>,
}

impl Compose {
    pub fn new(layer_count: u16, blend_modes: Vec<BlendMode>, opacities: Vec<f32>) -> Self {
        Self {
            layer_count,
            blend_modes,
            opacities,
            grid: HashMap::new(),
        }
    }
}

impl Processor for Compose {
    fn kind(&self) -> &'static str {
        "compose"
    }
    fn in_out_ports(&self) -> &'static InOutPortSpecification {
        &COMPOSE_PORTS
    }
    fn hints(&self) -> StageHints {
        StageHints::prefer_gpu()
    }

    fn process(&mut self, ctx: ProcessorContext<'_>, item: Item) -> Result<(), Error> {
        let tile = ProcessorContext::take_tile(item)?;

        let key = TileGridPos {
            mip_level: tile.coord.mip_level,
            tx: tile.coord.tx,
            ty: tile.coord.ty,
        };

        if ctx.port >= self.layer_count {
            return Err(Error::internal(format!(
                "Compose: port {} out of bounds (layer_count {})",
                ctx.port, self.layer_count,
            )));
        }
        let slots = self
            .grid
            .entry(key)
            .or_insert_with(|| vec![None; self.layer_count as usize]);
        slots[ctx.port as usize] = Some(tile);

        self.try_compose(key, ctx.device, ctx.gpu, ctx.emit);
        Ok(())
    }

    fn finish(&mut self, ctx: ProcessorContext<'_>) -> Result<(), Error> {
        let keys: Vec<TileGridPos> = self.grid.keys().cloned().collect();
        for key in keys {
            self.flush_slot(key, ctx.device, ctx.gpu.clone(), ctx.emit);
        }
        Ok(())
    }
}

impl Compose {
    fn try_compose(
        &mut self,
        key: TileGridPos,
        device: Device,
        gpu: Option<Arc<GpuContext>>,
        emit: &mut Emitter<Item>,
    ) {
        let Some(slots) = self.grid.get(&key) else {
            return;
        };
        if slots.iter().any(|s| s.is_none()) {
            return;
        }

        let tiles: Vec<(u16, Tile)> = self
            .grid
            .remove(&key)
            .unwrap()
            .into_iter()
            .enumerate()
            .filter_map(|(i, o)| o.map(|t| (i as u16, t)))
            .collect();

        self.compose_tiles(tiles, device, gpu, emit, "try_compose");
    }

    fn flush_slot(
        &mut self,
        key: TileGridPos,
        device: Device,
        gpu: Option<Arc<GpuContext>>,
        emit: &mut Emitter<Item>,
    ) {
        let Some(slots) = self.grid.remove(&key) else {
            return;
        };
        let tiles: Vec<(u16, Tile)> = slots
            .into_iter()
            .enumerate()
            .filter_map(|(i, o)| o.map(|t| (i as u16, t)))
            .collect();

        self.compose_tiles(tiles, device, gpu, emit, "flush_slot");
    }

    fn compose_tiles(
        &self,
        tiles: Vec<(u16, Tile)>,
        device: Device,
        gpu: Option<Arc<GpuContext>>,
        emit: &mut Emitter<Item>,
        site: &str,
    ) {
        if device == Device::Gpu
            && let Some(gpu_ctx) = gpu
        {
            match gpu_compose(&gpu_ctx, tiles, &self.blend_modes, &self.opacities, emit) {
                Ok(()) => return,
                Err((e, fallback_tiles)) => {
                    tracing::error!("GPU compose {site} failed ({e}), falling back to CPU");
                    cpu_compose(fallback_tiles, &self.blend_modes, &self.opacities, emit);
                    return;
                }
            }
        }
        cpu_compose(tiles, &self.blend_modes, &self.opacities, emit);
    }
}

// ── GPU path ──────────────────────────────────────────────────────────────────

fn gpu_compose(
    gpu_ctx: &Arc<GpuContext>,
    mut tiles: Vec<(u16, Tile)>,
    _blend_modes: &[BlendMode],
    opacities: &[f32],
    emit: &mut Emitter<Item>,
) -> Result<(), (Error, Vec<(u16, Tile)>)> {
    if tiles.is_empty() {
        return Ok(());
    }

    tiles.sort_by_key(|(port, _)| *port);

    // Validate all tiles are GPU before consuming them.
    for (_, tile) in &tiles {
        if tile.data.is_cpu() {
            return Err((
                Error::internal("compose GPU path received CPU tile — invariant violation"),
                tiles,
            ));
        }
    }

    let w = tiles.iter().map(|(_, t)| t.coord.width).max().unwrap();
    let h = tiles.iter().map(|(_, t)| t.coord.height).max().unwrap();
    let meta = tiles[0].1.meta;
    let mut out_coord = tiles[0].1.coord;
    out_coord.width = w;
    out_coord.height = h;

    let fmt = meta.format;
    let buf_size = w as u64 * h as u64 * fmt.bytes_per_pixel() as u64;
    let scheduler = gpu_ctx.scheduler();

    // Accumulator starts as zeroed transparent background.
    let mut acc: Arc<GpuBuffer> = Arc::new(scheduler.alloc_zeroed_buffer(buf_size));

    for (port, tile) in tiles {
        let layer_buf: Arc<GpuBuffer> = match tile.data {
            Buffer::Gpu(b) => b,
            Buffer::Cpu(_) => unreachable!("validated above"),
        };

        let opacity_b = opacities.get(port as usize).copied().unwrap_or(1.0);
        let kernel = pixors_shader::kernel::compose::ComposeParamsKernel::new(
            pixors_shader::kernel::compose::ComposeParams {
                width: w,
                height: h,
                opacity_a: 1.0,
                opacity_b,
            },
            fmt,
        );

        let out = scheduler.allocate_buffer(buf_size);
        let out = scheduler
            .dispatch_one(
                &kernel,
                &[&acc, &layer_buf],
                out,
                w.div_ceil(8),
                h.div_ceil(8),
            )
            .map_err(|e| {
                (
                    Error::internal(format!("GPU compose dispatch: {e}")),
                    Vec::new(),
                )
            })?;

        acc = Arc::new(out);
    }

    emit.emit(Item::Tile(Tile::new(out_coord, meta, Buffer::Gpu(acc))));
    Ok(())
}

// ── CPU path ──────────────────────────────────────────────────────────────────

fn cpu_compose(
    tiles: Vec<(u16, Tile)>,
    blend_modes: &[BlendMode],
    opacities: &[f32],
    emit: &mut Emitter<Item>,
) {
    if tiles.is_empty() {
        return;
    }

    let bpp = tiles[0].1.meta.format.bytes_per_pixel();
    let mut w = 0;
    let mut h = 0;
    for (_, tile) in tiles.iter() {
        w = w.max(tile.coord.width as usize);
        h = h.max(tile.coord.height as usize);
    }
    let meta = tiles[0].1.meta;
    let mut coord = tiles[0].1.coord;
    coord.width = w as u32;
    coord.height = h as u32;

    let mut out = vec![0u8; w * h * bpp];

    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * bpp;
            let mut result: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
            let mut started = false;

            for (port, tile) in tiles.iter() {
                if x >= tile.coord.width as usize || y >= tile.coord.height as usize {
                    continue;
                }
                let data = match &tile.data {
                    Buffer::Cpu(v) => v.as_slice(),
                    Buffer::Gpu(_) => continue,
                };
                let t_off = (y * tile.coord.width as usize + x) * bpp;
                if t_off + bpp > data.len() {
                    continue;
                }

                let mut pixel = read_pixel(&data[t_off..], bpp);
                let opacity = opacities.get(*port as usize).copied().unwrap_or(1.0);
                pixel[3] *= opacity;

                if !started {
                    result = pixel;
                    started = true;
                } else {
                    let mode = blend_modes.get(*port as usize).copied().unwrap_or_default();
                    alpha_over_f32(&pixel, &mut result, mode);
                }
            }

            write_pixel(result, bpp, &mut out[off..]);
        }
    }

    emit.emit(Item::Tile(Tile::new(coord, meta, Buffer::cpu(out))));
}

fn read_pixel(data: &[u8], bpp: usize) -> [f32; 4] {
    match bpp {
        4 => [
            data[0] as f32 / 255.0,
            data[1] as f32 / 255.0,
            data[2] as f32 / 255.0,
            data[3] as f32 / 255.0,
        ],
        8 => {
            let r = half::f16::from_le_bytes([data[0], data[1]]).to_f32();
            let g = half::f16::from_le_bytes([data[2], data[3]]).to_f32();
            let b = half::f16::from_le_bytes([data[4], data[5]]).to_f32();
            let a = half::f16::from_le_bytes([data[6], data[7]]).to_f32();
            [r, g, b, a]
        }
        _ => [0.0, 0.0, 0.0, 0.0],
    }
}

fn write_pixel(pixel: [f32; 4], bpp: usize, dst: &mut [u8]) {
    match bpp {
        4 => {
            dst[0] = (pixel[0].clamp(0.0, 1.0) * 255.0) as u8;
            dst[1] = (pixel[1].clamp(0.0, 1.0) * 255.0) as u8;
            dst[2] = (pixel[2].clamp(0.0, 1.0) * 255.0) as u8;
            dst[3] = (pixel[3].clamp(0.0, 1.0) * 255.0) as u8;
        }
        8 => {
            fn f32_to_f16(v: f32) -> half::f16 {
                half::f16::from_f32(v.clamp(-65504.0, 65504.0))
            }
            let r = f32_to_f16(pixel[0]).to_le_bytes();
            let g = f32_to_f16(pixel[1]).to_le_bytes();
            let b = f32_to_f16(pixel[2]).to_le_bytes();
            let a = f32_to_f16(pixel[3]).to_le_bytes();
            dst[0..2].copy_from_slice(&r);
            dst[2..4].copy_from_slice(&g);
            dst[4..6].copy_from_slice(&b);
            dst[6..8].copy_from_slice(&a);
        }
        _ => {}
    }
}

fn alpha_over_f32(top: &[f32; 4], result: &mut [f32; 4], mode: BlendMode) {
    match mode {
        BlendMode::Normal | BlendMode::Over => {
            let a_top = top[3];
            let a_bot = result[3];
            let inv_a = 1.0 - a_top;
            let a_out = a_top + a_bot * inv_a;
            if a_out <= 0.0 {
                *result = [0.0; 4];
                return;
            }
            let inv = 1.0 / a_out;
            result[0] = (top[0] * a_top + result[0] * a_bot * inv_a) * inv;
            result[1] = (top[1] * a_top + result[1] * a_bot * inv_a) * inv;
            result[2] = (top[2] * a_top + result[2] * a_bot * inv_a) * inv;
            result[3] = a_out;
        }
        BlendMode::Source => *result = *top,
    }
}
