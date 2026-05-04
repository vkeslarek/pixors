use std::sync::Arc;

use crate::data::{Buffer, GpuBuffer, Tile, TileCoord};
use crate::error::Error;
use crate::gpu;
use crate::gpu::GpuContext;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::model::pixel::meta::PixelMeta;
use crate::stage::{CpuKernel, GpuInputBinding, GpuKernelDescriptor};

use super::runner::{ItemReceiver, ItemSender, Runner};

// ── Chain step ──────────────────────────────────────────────────────────────

/// A single step in a GPU chain: either a GPU dispatch or a CPU kernel
/// (for Either stages that were promoted into the GPU chain).
pub enum ChainStep {
    Gpu(GpuKernelDescriptor),
    Cpu(Box<dyn CpuKernel>),
}

// ── GpuChainRunner ──────────────────────────────────────────────────────────

/// Runs an ordered list of steps (GPU dispatches + CPU kernels) in a single
/// thread. Items flow through the chain: each step may emit 0..N outputs.
pub struct GpuChainRunner {
    pub steps: Vec<ChainStep>,
}

impl GpuChainRunner {
    pub fn new(steps: Vec<ChainStep>) -> Self {
        Self { steps }
    }
}

impl Runner for GpuChainRunner {
    fn run(
        mut self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<ItemSender>,
    ) -> Result<(), Error> {
        let ctx = gpu::try_init()
            .ok_or_else(|| Error::internal("GpuChainRunner: no GPU available"))?;

        let recv = inputs.into_iter().next();
        let Some(recv) = recv else {
            send_eos(&outputs);
            return Ok(());
        };

        let steps = &mut self.steps;
        loop {
            match recv.recv() {
                Ok(Some(item)) => {
                    let items = run_chain(steps, item, &ctx)?;
                    ctx.scheduler().flush();
                    send_to_all(&outputs, items);
                }
                Ok(None) | Err(_) => {
                    let items = run_chain_finish(steps, &ctx)?;
                    ctx.scheduler().flush();
                    send_to_all(&outputs, items);
                    break;
                }
            }
        }

        send_eos(&outputs);
        Ok(())
    }
}

// ── Chain execution ─────────────────────────────────────────────────────────

/// Push a single item through every step in the chain, collecting final outputs.
fn run_chain(
    steps: &mut [ChainStep],
    item: Item,
    ctx: &Arc<GpuContext>,
) -> Result<Vec<Item>, Error> {
    let mut current = vec![item];
    for step in steps.iter_mut() {
        let mut next = Vec::new();
        match step {
            ChainStep::Gpu(desc) => {
                for it in current {
                    next.push(dispatch_gpu(desc, it, ctx)?);
                }
            }
            ChainStep::Cpu(kernel) => {
                for it in current {
                    let it = ensure_cpu(it, ctx)?;
                    let mut emit = Emitter::new();
                    kernel.process(it, &mut emit)?;
                    next.extend(emit.into_items());
                }
            }
        }
        current = next;
    }
    Ok(current)
}

/// Flush every CPU kernel's `finish()` and route the emitted items through
/// the remaining downstream steps.
fn run_chain_finish(
    steps: &mut [ChainStep],
    ctx: &Arc<GpuContext>,
) -> Result<Vec<Item>, Error> {
    let mut all: Vec<Item> = Vec::new();
    let n = steps.len();
    for i in 0..n {
        let (left, right) = steps.split_at_mut(i + 1);
        if let ChainStep::Cpu(kernel) = left.last_mut().unwrap() {
            let mut emit = Emitter::new();
            kernel.finish(&mut emit)?;
            let items = emit.into_items();
            if i + 1 < n {
                for it in items {
                    all.extend(run_chain(right, it, ctx)?);
                }
            } else {
                all.extend(items);
            }
        }
    }
    Ok(all)
}

// ── GPU dispatch ────────────────────────────────────────────────────────────

/// Dispatch a single item through a GPU kernel descriptor.
/// Handles both Tile and Neighborhood input bindings.
fn dispatch_gpu(
    desc: &GpuKernelDescriptor,
    item: Item,
    ctx: &Arc<GpuContext>,
) -> Result<Item, Error> {
    let scheduler = ctx.scheduler();

    // 1. Prepare input buffer.
    let (input_gbuf, coord, meta) = match desc.input_binding {
        GpuInputBinding::Tile => prepare_tile_input(&item, ctx)?,
        GpuInputBinding::Neighborhood => prepare_neighborhood_input(&item, ctx)?,
    };

    // 2. Write uniform params.
    if let Some(ref write_fn) = desc.write_params {
        let param_size = desc.param_size.max(16) as usize;
        let mut buf = vec![0u8; param_size];
        write_fn(&item, &mut buf);
        let param_buf = scheduler.pool().acquire(
            param_size as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        ctx.queue().write_buffer(&param_buf, 0, &buf);
    }

    // 3. Build a transient GpuKernel and dispatch.
    let out_size = (coord.width * coord.height * 4) as u64;
    let kernel = TransientKernel::from_desc(desc, &item);
    let dispatch_x = coord.width.div_ceil(desc.workgroup.0);
    let dispatch_y = coord.height.div_ceil(desc.workgroup.1);

    let out_arc = scheduler
        .dispatch_one(&kernel, &[&input_gbuf], out_size, dispatch_x, dispatch_y)
        .map_err(|e| Error::internal(format!("GPU dispatch: {e}")))?;

    Ok(Item::Tile(Tile::new(
        coord,
        meta,
        Buffer::Gpu(GpuBuffer::new(out_arc, out_size)),
    )))
}

// ── Input preparation ───────────────────────────────────────────────────────

/// Upload a Tile's data to GPU if it isn't there already.
fn prepare_tile_input(
    item: &Item,
    ctx: &Arc<GpuContext>,
) -> Result<(GpuBuffer, TileCoord, PixelMeta), Error> {
    let tile = match item {
        Item::Tile(t) => t,
        _ => return Err(Error::internal("dispatch_gpu: expected Tile")),
    };
    let gbuf = match &tile.data {
        Buffer::Gpu(g) => g.clone(),
        Buffer::Cpu(v) => upload_bytes(ctx, v.as_slice()),
    };
    Ok((gbuf, tile.coord, tile.meta))
}

/// Assemble a Neighborhood's tiles into a padded flat buffer and upload.
fn prepare_neighborhood_input(
    item: &Item,
    ctx: &Arc<GpuContext>,
) -> Result<(GpuBuffer, TileCoord, PixelMeta), Error> {
    let nbhd = match item {
        Item::Neighborhood(n) => n,
        _ => return Err(Error::internal("dispatch_gpu: expected Neighborhood")),
    };

    let r = nbhd.radius;
    let (cw, ch) = (nbhd.center.width, nbhd.center.height);
    let (cx, cy) = (nbhd.center.px, nbhd.center.py);
    let bpp = 4usize;
    let (pw, ph) = ((cw + 2 * r) as usize, (ch + 2 * r) as usize);
    let (ox, oy) = (cx.saturating_sub(r), cy.saturating_sub(r));

    let mut assembled = vec![0u8; pw * ph * bpp];

    for tile in &nbhd.tiles {
        let tile_data = match &tile.data {
            Buffer::Cpu(v) => v.as_slice(),
            Buffer::Gpu(_) => {
                return Err(Error::internal(
                    "dispatch_gpu: Neighborhood tiles must be CPU-backed for assembly",
                ))
            }
        };

        let tw = tile.coord.width as usize;
        let (tpx, tpy) = (tile.coord.px, tile.coord.py);

        // Intersection of the padded region and this tile.
        let x0 = ox.max(tpx);
        let y0 = oy.max(tpy);
        let x1 = (ox + pw as u32).min(tpx + tile.coord.width);
        let y1 = (oy + ph as u32).min(tpy + tile.coord.height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }

        let copy_w = (x1 - x0) as usize;
        for abs_y in y0..y1 {
            let src = ((abs_y - tpy) as usize * tw + (x0 - tpx) as usize) * bpp;
            let dst = ((abs_y - oy) as usize * pw + (x0 - ox) as usize) * bpp;
            let len = copy_w * bpp;
            if src + len <= tile_data.len() && dst + len <= assembled.len() {
                assembled[dst..dst + len].copy_from_slice(&tile_data[src..src + len]);
            }
        }
    }

    let gbuf = upload_bytes(ctx, &assembled);
    Ok((gbuf, nbhd.center, nbhd.meta))
}

/// Upload raw bytes into a GPU storage buffer.
fn upload_bytes(ctx: &Arc<GpuContext>, data: &[u8]) -> GpuBuffer {
    let size = data.len() as u64;
    let usage =
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let pool_buf = ctx.scheduler().pool().acquire(size, usage);
    let arc = pool_buf.arc();
    ctx.queue().write_buffer(&arc, 0, data);
    GpuBuffer::new(arc, size)
}

// ── GPU ↔ CPU transfer ──────────────────────────────────────────────────────

/// If the item is a Tile backed by a GPU buffer, download it to CPU.
/// Other item kinds pass through unchanged.
fn ensure_cpu(item: Item, ctx: &Arc<GpuContext>) -> Result<Item, Error> {
    let tile = match &item {
        Item::Tile(t) => t,
        _ => return Ok(item),
    };
    let gbuf = match tile.data.as_gpu() {
        Some(g) => g,
        None => return Ok(item),
    };

    ctx.scheduler().flush();

    let size = gbuf.size;
    let staging = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("chain-cpu-staging"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut enc = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("chain-cpu-copy"),
        });
    enc.copy_buffer_to_buffer(&gbuf.buffer, 0, &staging, 0, size);
    ctx.queue().submit(std::iter::once(enc.finish()));

    let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
    ctx.device().poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| Error::internal("ensure_cpu: recv"))?
        .map_err(|e| Error::internal(format!("ensure_cpu: map: {e:?}")))?;

    let bytes = staging.slice(..).get_mapped_range().to_vec();
    staging.unmap();
    Ok(Item::Tile(Tile::new(
        tile.coord,
        tile.meta,
        Buffer::cpu(bytes),
    )))
}

// ── Transient kernel wrapper ────────────────────────────────────────────────

/// Wraps a `GpuKernelDescriptor` + current `Item` to satisfy the
/// `GpuKernel` trait expected by `Scheduler::dispatch_one`.
struct TransientKernel<'a> {
    sig: crate::gpu::kernel::KernelSignature,
    item: &'a Item,
    write_fn: Option<&'a (dyn Fn(&Item, &mut [u8]) + Send + Sync)>,
}

impl<'a> TransientKernel<'a> {
    fn from_desc(desc: &'a GpuKernelDescriptor, item: &'a Item) -> Self {
        use crate::gpu::kernel::*;

        static INPUTS: &[ResourceDeclaration] = &[ResourceDeclaration {
            name: "input",
            element: BindingElement::PixelRgba8U32,
            access: BindingAccess::Read,
        }];
        static OUTPUTS: &[ResourceDeclaration] = &[ResourceDeclaration {
            name: "output",
            element: BindingElement::PixelRgba8U32,
            access: BindingAccess::Write,
        }];
        static PARAMS_4: &[ParameterDeclaration] = &[
            ParameterDeclaration { name: "width",  kind: ParameterType::U32 },
            ParameterDeclaration { name: "height", kind: ParameterType::U32 },
            ParameterDeclaration { name: "radius", kind: ParameterType::U32 },
            ParameterDeclaration { name: "_pad",   kind: ParameterType::U32 },
        ];

        let params = if desc.param_size > 0 { PARAMS_4 } else { &[] };

        Self {
            sig: KernelSignature {
                name: desc.entry_point,
                entry: desc.entry_point,
                inputs: INPUTS,
                outputs: OUTPUTS,
                params,
                workgroup: (desc.workgroup.0, desc.workgroup.1, 1),
                dispatch: DispatchShape::PerPixel,
                class: KernelClass::Custom,
                body: desc.spirv,
            },
            item,
            write_fn: desc.write_params.as_ref().map(|f| f.as_ref()),
        }
    }
}

impl crate::gpu::kernel::GpuKernel for TransientKernel<'_> {
    fn signature(&self) -> &crate::gpu::kernel::KernelSignature {
        &self.sig
    }
    fn write_params(&self, destination: &mut [u8]) {
        if let Some(f) = self.write_fn {
            f(self.item, destination);
        }
    }
}

// ── Channel helpers ─────────────────────────────────────────────────────────

fn send_to_all(outputs: &[ItemSender], items: Vec<Item>) {
    for item in items {
        for out in outputs {
            let _ = out.send(Some(item.clone()));
        }
    }
}

fn send_eos(outputs: &[ItemSender]) {
    for out in outputs {
        let _ = out.send(None);
    }
}
