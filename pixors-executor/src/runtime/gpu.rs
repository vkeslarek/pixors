use std::sync::Arc;

use crate::data::{Buffer, GpuBuffer, Tile};
use crate::error::Error;
use crate::gpu;
use crate::graph::item::Item;
use crate::stage::{GpuInputBinding, GpuKernelDescriptor};

use super::runner::{ItemReceiver, ItemSender, Runner};

/// GPU chain runner. Holds an ordered list of GPU kernel descriptors.
/// All kernels in the chain share one command encoder per item-batch for
/// elided submissions (one submit for the whole chain).
///
/// Neighborhood input: the runner assembles the padded region into a flat
/// CPU buffer and uploads it before the first dispatch in the chain.
pub struct GpuChainRunner {
    pub descriptors: Vec<GpuKernelDescriptor>,
}

impl GpuChainRunner {
    pub fn new(descriptors: Vec<GpuKernelDescriptor>) -> Self {
        Self { descriptors }
    }
}

impl Runner for GpuChainRunner {
    fn run(
        self: Box<Self>,
        inputs: Vec<ItemReceiver>,
        outputs: Vec<ItemSender>,
    ) -> Result<(), Error> {
        let ctx = gpu::try_init()
            .ok_or_else(|| Error::internal("GpuChainRunner: no GPU available"))?;

        let recv = inputs.into_iter().next();
        let Some(recv) = recv else {
            for out in &outputs {
                let _ = out.send(None);
            }
            return Ok(());
        };

        loop {
            match recv.recv() {
                Ok(Some(item)) => {
                    let out_item = dispatch_chain(&self.descriptors, item, &ctx)?;
                    ctx.scheduler().flush();
                    for out in &outputs {
                        let _ = out.send(Some(out_item.clone()));
                    }
                }
                Ok(None) | Err(_) => {
                    break;
                }
            }
        }

        for out in &outputs {
            let _ = out.send(None);
        }
        Ok(())
    }
}

fn dispatch_chain(
    descs: &[GpuKernelDescriptor],
    mut item: Item,
    ctx: &Arc<crate::gpu::GpuContext>,
) -> Result<Item, Error> {
    let scheduler = ctx.scheduler();

    for desc in descs {
        // --- Prepare input GPU buffer ---
        let (input_gbuf, coord, meta) = match desc.input_binding {
            GpuInputBinding::Tile => {
                let tile = match &item {
                    Item::Tile(t) => t,
                    _ => return Err(Error::internal("GpuChainRunner: expected Tile")),
                };
                let gbuf = match &tile.data {
                    Buffer::Gpu(g) => g.clone(),
                    Buffer::Cpu(v) => {
                        let size = v.len() as u64;
                        let usage = wgpu::BufferUsages::STORAGE
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::COPY_DST;
                        let pool_buf = scheduler.pool().acquire(size, usage);
                        let arc = pool_buf.arc();
                        ctx.queue().write_buffer(&arc, 0, v.as_slice());
                        GpuBuffer::new(arc, size)
                    }
                };
                (gbuf, tile.coord, tile.meta)
            }
            GpuInputBinding::Neighborhood => {
                let nbhd = match &item {
                    Item::Neighborhood(n) => n,
                    _ => return Err(Error::internal("GpuChainRunner: expected Neighborhood")),
                };
                let r = nbhd.radius;
                let cw = nbhd.center.width;
                let ch = nbhd.center.height;
                let cx = nbhd.center.px;
                let cy = nbhd.center.py;
                let bpp = 4usize;
                let rw = (cw + 2 * r) as usize;
                let rh = (ch + 2 * r) as usize;
                let rox = cx.saturating_sub(r);
                let roy = cy.saturating_sub(r);
                let mut assembled = vec![0u8; rw * rh * bpp];

                for tile in &nbhd.tiles {
                    let tile_data = match &tile.data {
                        Buffer::Cpu(v) => v.as_slice().to_vec(),
                        Buffer::Gpu(_) => {
                            return Err(Error::internal(
                                "GpuChainRunner: Neighborhood tiles must be on CPU for assembly",
                            ))
                        }
                    };
                    let tw = tile.coord.width as usize;
                    let tpx = tile.coord.px;
                    let tpy = tile.coord.py;
                    let x0 = rox.max(tpx);
                    let y0 = roy.max(tpy);
                    let x1 = (rox + rw as u32).min(tpx + tile.coord.width);
                    let y1 = (roy + rh as u32).min(tpy + tile.coord.height);
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    let copy_w = (x1 - x0) as usize;
                    for abs_y in y0..y1 {
                        let src_row = (abs_y - tpy) as usize;
                        let src_col = (x0 - tpx) as usize;
                        let dst_row = (abs_y - roy) as usize;
                        let dst_col = (x0 - rox) as usize;
                        let src_off = (src_row * tw + src_col) * bpp;
                        let dst_off = (dst_row * rw + dst_col) * bpp;
                        let len = copy_w * bpp;
                        if src_off + len > tile_data.len() || dst_off + len > assembled.len() {
                            continue;
                        }
                        assembled[dst_off..dst_off + len]
                            .copy_from_slice(&tile_data[src_off..src_off + len]);
                    }
                }

                let size = assembled.len() as u64;
                let usage = wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST;
                let pool_buf = scheduler.pool().acquire(size, usage);
                let arc = pool_buf.arc();
                ctx.queue().write_buffer(&arc, 0, &assembled);
                let gbuf = GpuBuffer::new(arc, size);
                (gbuf, nbhd.center, nbhd.meta)
            }
        };

        // --- Params ---
        let param_size = desc.param_size.max(16);
        let param_usage = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let param_buf = scheduler.pool().acquire(param_size, param_usage);
        if let Some(ref write_params) = desc.write_params {
            let mut params = vec![0u8; param_size as usize];
            write_params(&item, &mut params);
            ctx.queue().write_buffer(&param_buf, 0, &params);
        }

        // --- Output buffer ---
        let out_size = (coord.width * coord.height * 4) as u64;

        use crate::gpu::kernel::{
            BindingAccess, BindingElement, DispatchShape, KernelClass, KernelSignature,
            ResourceDeclaration,
        };

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
        static PARAMS_4: &[crate::gpu::kernel::ParameterDeclaration] = &[
            crate::gpu::kernel::ParameterDeclaration {
                name: "width",
                kind: crate::gpu::kernel::ParameterType::U32,
            },
            crate::gpu::kernel::ParameterDeclaration {
                name: "height",
                kind: crate::gpu::kernel::ParameterType::U32,
            },
            crate::gpu::kernel::ParameterDeclaration {
                name: "radius",
                kind: crate::gpu::kernel::ParameterType::U32,
            },
            crate::gpu::kernel::ParameterDeclaration {
                name: "_pad",
                kind: crate::gpu::kernel::ParameterType::U32,
            },
        ];

        let has_params = desc.param_size > 0;
        let params_slice = if has_params { PARAMS_4 } else { &[] };

        let sig = KernelSignature {
            name: desc.entry_point,
            entry: desc.entry_point,
            inputs: INPUTS,
            outputs: OUTPUTS,
            params: params_slice,
            workgroup: (desc.workgroup.0, desc.workgroup.1, 1),
            dispatch: DispatchShape::PerPixel,
            class: KernelClass::Custom,
            body: desc.spirv,
        };

        struct OtKernel<'a> {
            sig: KernelSignature,
            item_ref: &'a Item,
            write_fn: Option<&'a (dyn Fn(&Item, &mut [u8]) + Send + Sync)>,
        }

        impl<'a> crate::gpu::kernel::GpuKernel for OtKernel<'a> {
            fn signature(&self) -> &KernelSignature {
                &self.sig
            }
            fn write_params(&self, destination: &mut [u8]) {
                if let Some(f) = self.write_fn {
                    f(self.item_ref, destination);
                }
            }
        }

        let kernel = OtKernel {
            sig,
            item_ref: &item,
            write_fn: desc.write_params.as_ref().map(|f| f.as_ref()),
        };

        let dispatch_x = coord.width.div_ceil(desc.workgroup.0);
        let dispatch_y = coord.height.div_ceil(desc.workgroup.1);

        let out_arc = scheduler
            .dispatch_one(&kernel, &[&input_gbuf], out_size, dispatch_x, dispatch_y)
            .map_err(|e| Error::internal(format!("GPU dispatch: {e}")))?;

        let out_gbuf = GpuBuffer::new(out_arc, out_size);
        item = Item::Tile(Tile::new(coord, meta, Buffer::Gpu(out_gbuf)));
    }

    Ok(item)
}
