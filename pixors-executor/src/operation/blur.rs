use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::data::{Buffer, Device, Tile};
use crate::error::Error;
use crate::graph::emitter::Emitter;
use crate::graph::item::Item;
use crate::stage::{
    BufferAccess, CpuKernel, DataKind, GpuInputBinding, GpuKernelDescriptor, PortDecl, PortSpec,
    Stage, StageHints,
};

const BLUR_SPIRV: &[u8] = include_bytes!("../../kernels/blur.spv");

static BLUR_INPUTS: &[PortDecl] = &[PortDecl { name: "neighborhood", kind: DataKind::Neighborhood }];
static BLUR_OUTPUTS: &[PortDecl] = &[PortDecl { name: "tile", kind: DataKind::Tile }];
static BLUR_PORTS: PortSpec = PortSpec { inputs: BLUR_INPUTS, outputs: BLUR_OUTPUTS };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blur {
    pub radius: u32,
}

impl Stage for Blur {
    fn kind(&self) -> &'static str {
        "blur"
    }

    fn ports(&self) -> &'static PortSpec {
        &BLUR_PORTS
    }

    fn hints(&self) -> StageHints {
        StageHints {
            buffer_access: BufferAccess::ReadTransform,
            prefers_gpu: true,
        }
    }

    fn device(&self) -> Device { Device::Gpu }

    fn cpu_kernel(&self) -> Option<Box<dyn CpuKernel>> {
        Some(Box::new(BlurCpuRunner::new(self.radius)))
    }

    fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
        let radius = self.radius;
        Some(GpuKernelDescriptor {
            spirv: BLUR_SPIRV,
            entry_point: "cs_blur",
            input_binding: GpuInputBinding::Neighborhood,
            workgroup: (8, 8),
            param_size: 16,
            write_params: Some(Arc::new(move |item, dst| {
                let nbhd = match item {
                    crate::graph::item::Item::Neighborhood(n) => n,
                    _ => return,
                };
                // Shader expects **padded** dimensions (center + 2*radius).
                let params = BlurParams {
                    width: nbhd.center.width + 2 * radius,
                    height: nbhd.center.height + 2 * radius,
                    radius,
                    _pad: 0,
                };
                dst.copy_from_slice(bytemuck::bytes_of(&params));
            })),
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BlurParams {
    pub width: u32,
    pub height: u32,
    pub radius: u32,
    pub _pad: u32,
}

impl Blur {
    /// `width`/`height` are the **center** tile dimensions; we convert to
    /// padded dimensions here because the shader expects padded.
    pub fn write_params(&self, width: u32, height: u32, destination: &mut [u8]) {
        let params = BlurParams {
            width: width + 2 * self.radius,
            height: height + 2 * self.radius,
            radius: self.radius,
            _pad: 0,
        };
        destination.copy_from_slice(bytemuck::bytes_of(&params));
    }
}

// ── CPU Kernel ───────────────────────────────────────────────────────────────

use crate::debug_stopwatch;

pub struct BlurCpuRunner {
    radius: u32,
}

impl BlurCpuRunner {
    pub fn new(radius: u32) -> Self {
        Self { radius }
    }
}

impl CpuKernel for BlurCpuRunner {
    fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
        let _sw = debug_stopwatch!("blur");
        let nbhd = match item {
            Item::Neighborhood(n) => n,
            _ => return Err(Error::internal("expected Neighborhood")),
        };

        let cx = nbhd.center.px;
        let cy = nbhd.center.py;
        let cw = nbhd.center.width;
        let ch = nbhd.center.height;
        let r = self.radius;
        let bpp = 4usize;

        let rw = (cw + 2 * r) as usize;
        let rh = (ch + 2 * r) as usize;
        let rox = cx.saturating_sub(r);
        let roy = cy.saturating_sub(r);

        let mut src = vec![0u8; rw * rh * bpp];

        for tile in &nbhd.tiles {
            let tile_data: &[u8] = match &tile.data {
                Buffer::Cpu(v) => v.as_slice(),
                Buffer::Gpu(_) => return Err(Error::internal("GPU not supported")),
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

                if src_off + len > tile_data.len() || dst_off + len > src.len() {
                    continue;
                }
                src[dst_off..dst_off + len].copy_from_slice(&tile_data[src_off..src_off + len]);
            }
        }

        let blurred = box_blur_rgba8(&src, rw, rh, r as usize);

        let cw_u = cw as usize;
        let ch_u = ch as usize;
        let off_x = (cx - rox) as usize;
        let off_y = (cy - roy) as usize;
        let mut tile_data = Vec::with_capacity(cw_u * ch_u * bpp);
        for y in 0..ch_u {
            let row_off = ((off_y + y) * rw + off_x) * bpp;
            tile_data.extend_from_slice(&blurred[row_off..row_off + cw_u * bpp]);
        }

        emit.emit(Item::Tile(Tile::new(
            nbhd.center,
            nbhd.meta,
            Buffer::cpu(tile_data),
        )));
        Ok(())
    }
}

fn box_blur_rgba8(data: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if r == 0 {
        return data.to_vec();
    }

    let stride = w * 4;
    let hpass = blur_axis(data, h, stride, w, 4, r);
    blur_axis(&hpass, w, 4, h, stride, r)
}

fn blur_axis(
    data: &[u8],
    lines: usize,
    line_step: usize,
    axis_len: usize,
    step: usize,
    r: usize,
) -> Vec<u8> {
    let mut dst = vec![0u8; data.len()];

    for line in 0..lines {
        let line_origin = line * line_step;
        let mut sum = [0u32; 4];
        let mut count = 0u32;

        let initial_end = r.min(axis_len - 1);
        for i in 0..=initial_end {
            let off = line_origin + i * step;
            for c in 0..4 {
                sum[c] += data[off + c] as u32;
            }
            count += 1;
        }

        for i in 0..axis_len {
            if i > 0 {
                let new_i = i + r;
                if new_i < axis_len {
                    let off = line_origin + new_i * step;
                    for c in 0..4 {
                        sum[c] += data[off + c] as u32;
                    }
                    count += 1;
                }
                if i > r {
                    let old_i = i - r - 1;
                    let off = line_origin + old_i * step;
                    for c in 0..4 {
                        sum[c] -= data[off + c] as u32;
                    }
                    count -= 1;
                }
            }
            let dst_off = line_origin + i * step;
            for c in 0..4 {
                dst[dst_off + c] = (sum[c] / count) as u8;
            }
        }
    }

    dst
}
